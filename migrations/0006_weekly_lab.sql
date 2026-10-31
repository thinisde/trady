-- Offline lab and deployment-owned release boundary. Requires the NOINHERIT
-- login roles trady_lab, trady_verifier, trady_shadow, trady_releaser and
-- trady_release_owner to be provisioned by the administrator (0000_roles.sql).
CREATE SCHEMA weekly AUTHORIZATION trady_owner;
CREATE SCHEMA release AUTHORIZATION trady_owner;
REVOKE ALL ON SCHEMA weekly, release FROM PUBLIC;
GRANT CONNECT ON DATABASE trady TO trady_lab,trady_verifier,trady_shadow,trady_releaser,trady_release_owner;

CREATE TABLE weekly.budgets (
    account_id text NOT NULL, mode text NOT NULL, dataset_hash text NOT NULL,
    ceiling integer NOT NULL CHECK (ceiling BETWEEN 1 AND 10000),
    used integer NOT NULL DEFAULT 0 CHECK (used >= 0),
    PRIMARY KEY(account_id,mode,dataset_hash)
);
CREATE TABLE weekly.campaigns (
    account_id text NOT NULL, mode text NOT NULL, campaign_id text NOT NULL,
    dataset_hash text NOT NULL, ceiling integer NOT NULL,
    PRIMARY KEY(account_id,mode,campaign_id)
);
CREATE TABLE weekly.runs (
    run_id text PRIMARY KEY, document jsonb NOT NULL,
    worker text NOT NULL, backend integer NOT NULL,
    generation bigint NOT NULL DEFAULT 1, lease_until timestamptz NOT NULL
);
CREATE TABLE weekly.trials (
    run_id text NOT NULL REFERENCES weekly.runs, candidate_id text NOT NULL,
    document jsonb NOT NULL, PRIMARY KEY(run_id,candidate_id)
);
CREATE TABLE weekly.holdouts (
    run_id text PRIMARY KEY REFERENCES weekly.runs, candidate_id text NOT NULL,
    account_id text NOT NULL, mode text NOT NULL, dataset_hash text NOT NULL,
    input_hash text NOT NULL, consumed_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    report jsonb, UNIQUE(account_id,mode,dataset_hash)
);
CREATE TABLE weekly.forecasts (
    account_id text NOT NULL, mode text NOT NULL, forecast_id text NOT NULL,
    canonical text NOT NULL, content_hash text NOT NULL,
    recorded_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    PRIMARY KEY(account_id,mode,forecast_id)
);
CREATE TABLE weekly.labels (
    account_id text NOT NULL, mode text NOT NULL, forecast_id text NOT NULL,
    canonical text NOT NULL, content_hash text NOT NULL,
    recorded_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    PRIMARY KEY(account_id,mode,forecast_id),
    FOREIGN KEY(account_id,mode,forecast_id) REFERENCES weekly.forecasts
);
CREATE TABLE weekly.reconciliations (
    account_id text NOT NULL, mode text NOT NULL, export_id text NOT NULL,
    canonical text NOT NULL, content_hash text NOT NULL,
    recorded_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    PRIMARY KEY(account_id,mode,export_id)
);

CREATE FUNCTION weekly.hash(p_text text) RETURNS text LANGUAGE sql IMMUTABLE
SET search_path=pg_catalog AS $$ SELECT encode(sha256(convert_to(p_text,'UTF8')),'hex') $$;
CREATE FUNCTION weekly.require_fields(p_document jsonb,p_fields text[]) RETURNS void
LANGUAGE plpgsql SET search_path=pg_catalog AS $$
DECLARE field text;
BEGIN
    IF jsonb_typeof(p_document) IS DISTINCT FROM 'object' THEN RAISE EXCEPTION 'DOCUMENT_OBJECT_REQUIRED'; END IF;
    FOREACH field IN ARRAY p_fields LOOP
        IF NOT p_document ? field OR p_document->field='null'::jsonb THEN RAISE EXCEPTION 'REQUIRED_FIELD_MISSING: %',field; END IF;
    END LOOP;
END $$;

CREATE FUNCTION weekly.assert_fence(p_id text,p_generation bigint) RETURNS weekly.runs
LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog,weekly AS $$
DECLARE r weekly.runs;
BEGIN
    SELECT * INTO STRICT r FROM weekly.runs WHERE run_id=p_id FOR UPDATE;
    IF r.backend<>pg_backend_pid() OR r.generation<>p_generation OR r.lease_until<=clock_timestamp() THEN
        RAISE EXCEPTION 'WEEKLY_LEASE_LOST';
    END IF;
    RETURN r;
END $$;

CREATE FUNCTION weekly.freeze(p_id text,p_state jsonb,p_worker text,p_ttl integer)
RETURNS bigint LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog,weekly AS $$
DECLARE r weekly.runs; m jsonb:=p_state->'manifest'; plan jsonb:=p_state->'plan';
        v_account text:=m->>'account_id'; v_mode text:=m->>'mode'; data text:=m->>'dataset_hash';
        cap integer:=(plan->>'max_cumulative_trials')::integer; c weekly.campaigns;
BEGIN
    PERFORM weekly.require_fields(p_state,ARRAY['started_at','manifest','plan','policy','trials']);
    PERFORM weekly.require_fields(m,ARRAY['account_id','mode','dataset_hash']);
    PERFORM weekly.require_fields(plan,ARRAY['campaign_id','max_cumulative_trials']);
    IF NOT (p_state ?& ARRAY['reconciliation','verification','evaluation','decision']) THEN RAISE EXCEPTION 'WEEKLY_RESULT_FIELDS_MISSING'; END IF;
    IF p_id !~ '^[A-Za-z0-9_.:-]{1,160}$' OR length(p_worker) NOT BETWEEN 1 AND 160
       OR p_ttl NOT BETWEEN 1 AND 3600 OR cap NOT BETWEEN 1 AND 10000
       OR v_account IS NULL OR v_mode NOT IN ('paper','shadow','live') OR data !~ '^[a-f0-9]{64}$'
       OR jsonb_typeof(p_state->'trials')<>'array' THEN RAISE EXCEPTION 'INVALID_WEEKLY_FREEZE'; END IF;
    PERFORM pg_advisory_xact_lock(hashtextextended(p_id,6006));
    SELECT * INTO r FROM weekly.runs WHERE run_id=p_id FOR UPDATE;
    IF FOUND THEN
        IF r.document->'manifest'<>m OR r.document->'plan'<>plan OR r.document->'policy'<>p_state->'policy' THEN
            RAISE EXCEPTION 'RESUMED_RUN_INPUTS_CHANGED'; END IF;
        IF r.lease_until>clock_timestamp() AND (r.backend<>pg_backend_pid() OR r.worker<>p_worker) THEN
            RAISE EXCEPTION 'WEEKLY_RUN_BUSY'; END IF;
        UPDATE weekly.runs SET generation=generation+1,worker=p_worker,backend=pg_backend_pid(),
            lease_until=clock_timestamp()+make_interval(secs=>p_ttl) WHERE run_id=p_id RETURNING * INTO r;
        RETURN r.generation;
    END IF;
    IF p_state->'reconciliation'<>'null'::jsonb OR p_state->'verification'<>'null'::jsonb
       OR p_state->'evaluation'<>'null'::jsonb OR p_state->'decision'<>'null'::jsonb
       OR jsonb_array_length(p_state->'trials')<>0 THEN RAISE EXCEPTION 'PREPOPULATED_WEEKLY_RUN'; END IF;
    INSERT INTO weekly.budgets(account_id,mode,dataset_hash,ceiling) VALUES(v_account,v_mode,data,cap)
        ON CONFLICT(account_id,mode,dataset_hash) DO UPDATE SET ceiling=least(weekly.budgets.ceiling,excluded.ceiling);
    INSERT INTO weekly.campaigns VALUES(v_account,v_mode,plan->>'campaign_id',data,cap) ON CONFLICT DO NOTHING;
    SELECT * INTO STRICT c FROM weekly.campaigns WHERE account_id=v_account AND weekly.campaigns.mode=v_mode
        AND campaign_id=plan->>'campaign_id' FOR UPDATE;
    IF c.dataset_hash<>data OR c.ceiling<>cap THEN RAISE EXCEPTION 'CAMPAIGN_REBINDING_OR_BUDGET_CHANGE'; END IF;
    INSERT INTO weekly.runs VALUES(p_id,jsonb_set(p_state,'{started_at}',to_jsonb(clock_timestamp())),
        p_worker,pg_backend_pid(),1,clock_timestamp()+make_interval(secs=>p_ttl));
    RETURN 1;
END $$;

CREATE FUNCTION weekly.load_run(p_id text,p_generation bigint) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog,weekly AS $$
BEGIN RETURN (weekly.assert_fence(p_id,p_generation)).document; END $$;
CREATE FUNCTION weekly.renew(p_id text,p_generation bigint,p_ttl integer) RETURNS void
LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog,weekly AS $$
BEGIN
    PERFORM weekly.assert_fence(p_id,p_generation);
    IF p_ttl NOT BETWEEN 1 AND 3600 THEN RAISE EXCEPTION 'INVALID_LEASE_TTL'; END IF;
    UPDATE weekly.runs SET lease_until=clock_timestamp()+make_interval(secs=>p_ttl) WHERE run_id=p_id;
END $$;
CREATE FUNCTION weekly.release_run(p_id text,p_generation bigint) RETURNS void
LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog,weekly AS $$
BEGIN
    PERFORM weekly.assert_fence(p_id,p_generation);
    UPDATE weekly.runs SET lease_until=clock_timestamp() WHERE run_id=p_id;
END $$;

CREATE FUNCTION weekly.reserve_trial(p_id text,p_generation bigint,p_trial jsonb,p_limit integer) RETURNS void
LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog,weekly AS $$
DECLARE r weekly.runs; m jsonb; plan jsonb; previous jsonb; b weekly.budgets;
BEGIN
    r:=weekly.assert_fence(p_id,p_generation); m:=r.document->'manifest'; plan:=r.document->'plan';
    PERFORM weekly.require_fields(p_trial,ARRAY['candidate_id','parameters','registered_at']);
    IF NOT (p_trial ?& ARRAY['development','failure']) THEN RAISE EXCEPTION 'TRIAL_RESULT_FIELDS_MISSING'; END IF;
    IF p_limit<>(plan->>'max_cumulative_trials')::integer OR p_trial->>'candidate_id' IS NULL
       OR p_trial->'development'<>'null'::jsonb OR p_trial->'failure'<>'null'::jsonb
       OR NOT EXISTS(SELECT 1 FROM jsonb_array_elements(plan->'candidates') v WHERE v=p_trial->'parameters')
       OR r.document->'decision'<>'null'::jsonb THEN RAISE EXCEPTION 'INVALID_TRIAL_RESERVATION'; END IF;
    SELECT document INTO previous FROM weekly.trials WHERE run_id=p_id AND candidate_id=p_trial->>'candidate_id';
    IF FOUND THEN
        IF previous<>p_trial THEN RAISE EXCEPTION 'TRIAL_ID_REUSED'; END IF;
        RETURN;
    END IF;
    SELECT * INTO STRICT b FROM weekly.budgets WHERE account_id=m->>'account_id'
        AND mode=m->>'mode' AND dataset_hash=m->>'dataset_hash' FOR UPDATE;
    IF b.used>=b.ceiling THEN RAISE EXCEPTION 'CUMULATIVE_TRIAL_BUDGET_EXHAUSTED'; END IF;
    UPDATE weekly.budgets SET used=used+1 WHERE account_id=b.account_id AND mode=b.mode AND dataset_hash=b.dataset_hash;
    INSERT INTO weekly.trials VALUES(p_id,p_trial->>'candidate_id',p_trial);
    UPDATE weekly.runs SET document=jsonb_set(document,'{trials}',(document->'trials')||jsonb_build_array(p_trial)) WHERE run_id=p_id;
END $$;

CREATE FUNCTION weekly.checkpoint(p_id text,p_generation bigint,p_state jsonb) RETURNS void
LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog,weekly AS $$
DECLARE r weekly.runs; old jsonb; t jsonb; prior jsonb; field text; saved jsonb;
BEGIN
    r:=weekly.assert_fence(p_id,p_generation); old:=r.document;
    PERFORM weekly.require_fields(p_state,ARRAY['manifest','plan','policy','started_at','trials']);
    IF NOT (p_state ?& ARRAY['reconciliation','verification','evaluation','decision']) THEN RAISE EXCEPTION 'WEEKLY_RESULT_FIELDS_MISSING'; END IF;
    IF (p_state->>'started_at')::timestamptz IS DISTINCT FROM (old->>'started_at')::timestamptz THEN RAISE EXCEPTION 'FROZEN_START_TIME_CHANGED'; END IF;
    FOREACH field IN ARRAY ARRAY['manifest','plan','policy'] LOOP
        IF p_state->field IS DISTINCT FROM old->field THEN RAISE EXCEPTION 'FROZEN_WEEKLY_INPUT_CHANGED'; END IF;
    END LOOP;
    FOREACH field IN ARRAY ARRAY['reconciliation','verification','evaluation','decision'] LOOP
        IF old->field<>'null'::jsonb AND p_state->field IS DISTINCT FROM old->field THEN
            RAISE EXCEPTION 'COMPLETED_WEEKLY_EVIDENCE_CHANGED'; END IF;
    END LOOP;
    IF jsonb_array_length(p_state->'trials')<>jsonb_array_length(old->'trials') THEN RAISE EXCEPTION 'UNRESERVED_OR_REMOVED_TRIAL'; END IF;
    FOR t IN SELECT * FROM jsonb_array_elements(p_state->'trials') LOOP
        PERFORM weekly.require_fields(t,ARRAY['candidate_id','parameters','registered_at']);
        IF NOT (t ?& ARRAY['development','failure']) THEN RAISE EXCEPTION 'TRIAL_RESULT_FIELDS_MISSING'; END IF;
        SELECT v INTO prior FROM jsonb_array_elements(old->'trials') v WHERE v->>'candidate_id'=t->>'candidate_id';
        IF NOT FOUND OR t->'parameters' IS DISTINCT FROM prior->'parameters' OR t->'registered_at' IS DISTINCT FROM prior->'registered_at'
           OR (prior->'development'<>'null'::jsonb AND t->'development' IS DISTINCT FROM prior->'development')
           OR (prior->'failure'<>'null'::jsonb AND t->'failure' IS DISTINCT FROM prior->'failure')
           OR (t->'development'<>'null'::jsonb AND t->'failure'<>'null'::jsonb) THEN
            RAISE EXCEPTION 'IMMUTABLE_TRIAL_CHANGED'; END IF;
    END LOOP;
    IF (SELECT count(DISTINCT v->>'candidate_id') FROM jsonb_array_elements(p_state->'trials') v)
       <>jsonb_array_length(p_state->'trials') THEN RAISE EXCEPTION 'DUPLICATE_TRIAL'; END IF;
    IF p_state->'evaluation'<>'null'::jsonb THEN
        SELECT report INTO saved FROM weekly.holdouts WHERE run_id=p_id;
        IF saved IS DISTINCT FROM p_state->'evaluation' THEN RAISE EXCEPTION 'UNREGISTERED_HOLDOUT_REPORT'; END IF;
    END IF;
    UPDATE weekly.runs SET document=p_state WHERE run_id=p_id;
END $$;

CREATE FUNCTION weekly.claim_holdout(p_id text,p_generation bigint,p_candidate text,p_hash text) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog,weekly AS $$
DECLARE r weekly.runs; h weekly.holdouts;
BEGIN
    r:=weekly.assert_fence(p_id,p_generation);
    IF p_hash !~ '^[a-f0-9]{64}$' OR r.document->'verification'='null'::jsonb
       OR r.document->'verification'->'proof'->>'input_hash' IS DISTINCT FROM p_hash
       OR r.document->'verification'->'parity'->>'input_hash' IS DISTINCT FROM p_hash
       OR r.document->'verification'->>'approved_axioms_only' IS DISTINCT FROM 'true'
       OR r.document->'verification'->>'non_vacuous' IS DISTINCT FROM 'true'
       OR r.document->'verification'->>'exact_numeric_parity' IS DISTINCT FROM 'true'
       OR r.document->'verification'->>'safety_definitions_unchanged' IS DISTINCT FROM 'true'
       OR NOT EXISTS(SELECT 1 FROM jsonb_array_elements(r.document->'trials') t WHERE t->>'candidate_id'=p_candidate
           AND t->'development'<>'null'::jsonb AND t->'failure'='null'::jsonb) THEN
        RAISE EXCEPTION 'HOLDOUT_REQUIRES_FROZEN_VERIFIED_CANDIDATE'; END IF;
    SELECT * INTO h FROM weekly.holdouts WHERE run_id=p_id;
    IF FOUND THEN
        IF h.candidate_id<>p_candidate OR h.input_hash<>p_hash THEN RAISE EXCEPTION 'HOLDOUT_CANDIDATE_REBOUND'; END IF;
        IF h.report IS NULL THEN RAISE EXCEPTION 'INTERRUPTED_HOLDOUT_NOT_REPLAYED'; END IF;
        RETURN h.report;
    END IF;
    IF EXISTS(SELECT 1 FROM weekly.holdouts WHERE account_id=r.document->'manifest'->>'account_id'
        AND mode=r.document->'manifest'->>'mode' AND dataset_hash=r.document->'manifest'->>'dataset_hash') THEN
        RAISE EXCEPTION 'SEALED_DATASET_ALREADY_CONSUMED'; END IF;
    INSERT INTO weekly.holdouts(run_id,candidate_id,account_id,mode,dataset_hash,input_hash)
        VALUES(p_id,p_candidate,r.document->'manifest'->>'account_id',r.document->'manifest'->>'mode',r.document->'manifest'->>'dataset_hash',p_hash);
    RETURN NULL;
END $$;
CREATE FUNCTION weekly.finish_holdout(p_id text,p_generation bigint,p_report jsonb) RETURNS void
LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog,weekly AS $$
DECLARE h weekly.holdouts;
BEGIN
    PERFORM weekly.assert_fence(p_id,p_generation);
    SELECT * INTO STRICT h FROM weekly.holdouts WHERE run_id=p_id FOR UPDATE;
    IF p_report->'report'->>'input_hash'<>h.input_hash OR p_report->>'run_id'<>p_id
       OR (h.report IS NOT NULL AND h.report<>p_report) THEN RAISE EXCEPTION 'HOLDOUT_REPORT_REBOUND'; END IF;
    UPDATE weekly.holdouts SET report=p_report WHERE run_id=p_id;
END $$;

CREATE FUNCTION weekly.record_ledger(p_kind text,p_account text,p_mode text,p_id text,p_canonical text) RETURNS text
LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog,weekly AS $$
DECLARE d jsonb:=p_canonical::jsonb; h text:=weekly.hash(p_canonical); previous text; f jsonb;
BEGIN
    IF p_account !~ '^(paper|shadow|live)_[A-Za-z0-9_-]+$' OR p_mode NOT IN ('paper','shadow','live')
       OR lower(p_mode)<>split_part(p_account,'_',1) OR p_id !~ '^[A-Za-z0-9_.:-]{1,160}$'
       OR octet_length(p_canonical)>4194304 THEN RAISE EXCEPTION 'INVALID_LEDGER_SCOPE'; END IF;
    PERFORM pg_advisory_xact_lock(hashtextextended(p_account||p_mode||p_kind||p_id,6006));
    IF p_kind='forecast' THEN
        IF session_user NOT IN ('trady_analyst','trady_shadow') OR d->'metadata'->>'prediction_id'<>p_id
           OR (d->'metadata'->>'generated_at')::timestamptz>clock_timestamp()
           OR (d->'metadata'->'horizon'->>'deadline_at')::timestamptz<=clock_timestamp() THEN
            RAISE EXCEPTION 'FORECAST_NOT_PROSPECTIVE'; END IF;
        SELECT canonical INTO previous FROM weekly.forecasts WHERE account_id=p_account AND mode=p_mode AND forecast_id=p_id;
        IF FOUND AND previous<>p_canonical THEN RAISE EXCEPTION 'FORECAST_IMMUTABLE'; END IF;
        INSERT INTO weekly.forecasts VALUES(p_account,p_mode,p_id,p_canonical,h,clock_timestamp()) ON CONFLICT DO NOTHING;
    ELSIF p_kind='label' THEN
        IF session_user<>'trady_verifier' THEN RAISE EXCEPTION 'INDEPENDENT_LABELER_REQUIRED'; END IF;
        SELECT canonical::jsonb INTO STRICT f FROM weekly.forecasts WHERE account_id=p_account AND mode=p_mode AND forecast_id=p_id;
        IF d->>'forecast_hash'<>(SELECT content_hash FROM weekly.forecasts WHERE account_id=p_account AND mode=p_mode AND forecast_id=p_id)
           OR (d->>'observed_through')::timestamptz<(f->'metadata'->'horizon'->>'deadline_at')::timestamptz
           OR (d->>'observed_through')::timestamptz>clock_timestamp() THEN RAISE EXCEPTION 'UNMATURED_OR_MISBOUND_LABEL'; END IF;
        SELECT canonical INTO previous FROM weekly.labels WHERE account_id=p_account AND mode=p_mode AND forecast_id=p_id;
        IF FOUND AND previous<>p_canonical THEN RAISE EXCEPTION 'LABEL_IMMUTABLE'; END IF;
        INSERT INTO weekly.labels VALUES(p_account,p_mode,p_id,p_canonical,h,clock_timestamp()) ON CONFLICT DO NOTHING;
    ELSIF p_kind='reconciliation' THEN
        IF session_user<>'trady_verifier' OR d->>'account_id'<>p_account OR d->>'mode'<>p_mode
           OR d->>'export_id'<>p_id THEN RAISE EXCEPTION 'INDEPENDENT_RECONCILIATION_REQUIRED'; END IF;
        SELECT canonical INTO previous FROM weekly.reconciliations WHERE account_id=p_account AND mode=p_mode AND export_id=p_id;
        IF FOUND AND previous<>p_canonical THEN RAISE EXCEPTION 'RECONCILIATION_IMMUTABLE'; END IF;
        INSERT INTO weekly.reconciliations VALUES(p_account,p_mode,p_id,p_canonical,h,clock_timestamp()) ON CONFLICT DO NOTHING;
    ELSE RAISE EXCEPTION 'UNKNOWN_LEDGER_KIND'; END IF;
    RETURN h;
END $$;
CREATE FUNCTION weekly.export_ledger(p_account text,p_mode text,p_cutoff timestamptz) RETURNS jsonb
LANGUAGE sql SECURITY DEFINER SET search_path=pg_catalog,weekly AS $$
    SELECT jsonb_build_object('account_id',p_account,'mode',p_mode,'cutoff_at',p_cutoff,
      'forecasts',coalesce((SELECT jsonb_agg(jsonb_build_object('content',f.canonical::jsonb,'hash',f.content_hash,
          'recorded_at',f.recorded_at,'label',l.canonical::jsonb,'label_hash',l.content_hash) ORDER BY f.forecast_id)
        FROM weekly.forecasts f LEFT JOIN weekly.labels l ON (l.account_id=f.account_id AND l.mode=f.mode AND l.forecast_id=f.forecast_id AND l.recorded_at<=p_cutoff)
        WHERE f.account_id=p_account AND f.mode=p_mode AND f.recorded_at<=p_cutoff
          ),'[]'::jsonb),
      'reconciliations',coalesce((SELECT jsonb_agg(jsonb_build_object('content',canonical::jsonb,'hash',content_hash) ORDER BY export_id)
        FROM weekly.reconciliations WHERE account_id=p_account AND mode=p_mode AND recorded_at<=p_cutoff),'[]'::jsonb))
$$;

CREATE TABLE release.candidates (
    release_id text PRIMARY KEY, subject_hash text UNIQUE NOT NULL, canonical text NOT NULL,
    state text NOT NULL DEFAULT 'CANDIDATE' CHECK(state IN ('CANDIDATE','VERIFIED','SHADOW','APPROVED','ACTIVE')),
    evidence jsonb NOT NULL DEFAULT '{}'::jsonb, revision bigint NOT NULL DEFAULT 0,
    updated_at timestamptz NOT NULL DEFAULT clock_timestamp()
);
CREATE TABLE release.evidence (
    evidence_id text PRIMARY KEY, subject_hash text NOT NULL REFERENCES release.candidates(subject_hash),
    kind text NOT NULL CHECK(kind IN ('verification','calendar','session','owner','readiness')),
    canonical text NOT NULL, content_hash text NOT NULL, producer text NOT NULL,
    recorded_at timestamptz NOT NULL DEFAULT clock_timestamp()
);
CREATE UNIQUE INDEX release_distinct_sessions ON release.evidence(subject_hash,
    ((canonical::jsonb)->>'exchange'),((canonical::jsonb)->>'session_id')) WHERE kind='session';
CREATE TABLE release.events (
    release_id text NOT NULL REFERENCES release.candidates, revision bigint NOT NULL,
    state text NOT NULL, evidence jsonb NOT NULL, actor text NOT NULL, at timestamptz NOT NULL DEFAULT clock_timestamp(),
    PRIMARY KEY(release_id,revision)
);
CREATE TABLE release.active (
    account_id text NOT NULL, mode text NOT NULL, release_id text NOT NULL REFERENCES release.candidates,
    PRIMARY KEY(account_id,mode)
);
CREATE FUNCTION release.candidate(p_canonical text) RETURNS text
LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog,weekly,release AS $$
DECLARE d jsonb:=p_canonical::jsonb; h text:=weekly.hash(p_canonical); id text:='release_'||h;
BEGIN
    PERFORM weekly.require_fields(d,ARRAY['revision_id','account_id','mode','owner_id','policy_hash','formula_hash','source_hash',
        'calendar_hash','model_hashes','parameters','envelope','effective_from','valid_until']);
    IF d->>'account_id' !~ '^(paper|shadow|live)_[A-Za-z0-9_-]+$'
       OR lower(d->>'mode')<>split_part(d->>'account_id','_',1)
       OR d->>'owner_id' IS NULL OR d->>'policy_hash' !~ '^[a-f0-9]{64}$'
       OR d->>'formula_hash' !~ '^[a-f0-9]{64}$' OR d->>'source_hash' !~ '^[a-f0-9]{64}$'
       OR d->>'calendar_hash' !~ '^[a-f0-9]{64}$' OR octet_length(p_canonical)>4194304 THEN
        RAISE EXCEPTION 'INVALID_RELEASE_CANDIDATE'; END IF;
    IF d->>'mode' NOT IN ('paper','shadow','live') OR (d->>'effective_from')::timestamptz>=(d->>'valid_until')::timestamptz
        OR jsonb_typeof(d->'model_hashes')<>'object' OR d->'model_hashes'='{}'::jsonb THEN RAISE EXCEPTION 'INVALID_RELEASE_SCOPE'; END IF;
    INSERT INTO release.candidates(release_id,subject_hash,canonical) VALUES(id,h,p_canonical) ON CONFLICT DO NOTHING;
    RETURN id;
END $$;
CREATE FUNCTION release.record_evidence(p_kind text,p_canonical text) RETURNS text
LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog,weekly,release AS $$
DECLARE d jsonb:=p_canonical::jsonb; h text:=weekly.hash(p_canonical); id text:='evidence_'||h;
        candidate jsonb; c release.candidates; calendar jsonb; interval jsonb;
BEGIN
    PERFORM weekly.require_fields(d,ARRAY['subject_hash','scope_hash','kind','recorded_at']);
    SELECT * INTO STRICT c FROM release.candidates WHERE subject_hash=d->>'subject_hash' FOR UPDATE;
    candidate:=c.canonical::jsonb;
    IF octet_length(p_canonical)>4194304 OR d->>'kind'<>p_kind OR d->>'scope_hash'<>weekly.hash(c.canonical)
       OR (d->>'recorded_at')::timestamptz>clock_timestamp() THEN RAISE EXCEPTION 'MISBOUND_RELEASE_EVIDENCE'; END IF;
    IF p_kind IN ('verification','calendar','readiness') THEN
        IF session_user<>'trady_verifier' THEN RAISE EXCEPTION 'INDEPENDENT_VERIFIER_REQUIRED'; END IF;
        PERFORM weekly.require_fields(d,ARRAY['evidence_class']);
        IF d->>'evidence_class'<>'PRODUCTION' THEN RAISE EXCEPTION 'TEST_OR_DEMO_EVIDENCE_CANNOT_VERIFY_RELEASE'; END IF;
        IF p_kind='verification' THEN
            PERFORM weekly.require_fields(d,ARRAY['proof_hash','parity_hash','evaluation_hash','verifier_hash','source_hash','passed',
                'proof_report_id','parity_report_id','evaluation_report_id','evidence_class','scope','checks']);
            IF d->>'evidence_class'<>'PRODUCTION' OR d->>'scope'<>'FORMULA_V1' OR NOT (d->'checks' @>
                '{"clean_build":true,"approved_axioms":true,"non_vacuity":true,"full_numeric_parity":true,"protected_safety":true,"sealed_evaluation":true,"calibration":true,"schema_compatibility":true,"execution_protection":true,"source_permissions":true}'::jsonb)
                THEN RAISE EXCEPTION 'TEST_OR_DEMO_EVIDENCE_CANNOT_VERIFY_RELEASE'; END IF;
        ELSIF p_kind='calendar' THEN
            PERFORM weekly.require_fields(d,ARRAY['calendar_hash','sessions','sessions_canonical']);
            IF (d->>'sessions_canonical')::jsonb<>d->'sessions' OR weekly.hash(d->>'sessions_canonical')<>d->>'calendar_hash' THEN
                RAISE EXCEPTION 'CALENDAR_CONTENT_HASH_MISMATCH'; END IF;
            IF EXISTS(SELECT 1 FROM (SELECT v,lag((v->>'closes_at')::timestamptz) OVER(PARTITION BY v->>'exchange' ORDER BY (v->>'opens_at')::timestamptz) previous_close
                FROM jsonb_array_elements(d->'sessions') v) ordered WHERE NOT (v ?& ARRAY['session_id','exchange','opens_at','closes_at'])
                OR (v->>'opens_at')::timestamptz>=(v->>'closes_at')::timestamptz
                OR (v->>'opens_at')::timestamptz<=previous_close)
                OR (SELECT count(DISTINCT (v->>'exchange',v->>'session_id')) FROM jsonb_array_elements(d->'sessions') v)<>jsonb_array_length(d->'sessions') THEN
                RAISE EXCEPTION 'INVALID_OR_DUPLICATE_CALENDAR_SESSION'; END IF;
        ELSE
            PERFORM weekly.require_fields(d,ARRAY['ready','valid_until','account_id','mode','snapshot_hash','capabilities_hash']);
            IF d->>'snapshot_hash' !~ '^[a-f0-9]{64}$' OR d->>'capabilities_hash' !~ '^[a-f0-9]{64}$'
                OR (d->>'valid_until')::timestamptz>clock_timestamp()+interval '5 minutes' THEN RAISE EXCEPTION 'INVALID_READINESS_EVIDENCE'; END IF;
        END IF;
        IF p_kind='verification' AND (d->>'proof_hash' !~ '^[a-f0-9]{64}$' OR d->>'parity_hash' !~ '^[a-f0-9]{64}$'
            OR d->>'evaluation_hash' !~ '^[a-f0-9]{64}$' OR d->>'verifier_hash' !~ '^[a-f0-9]{64}$'
            OR d->>'source_hash'<>candidate->>'source_hash' OR NOT (d->>'passed')::boolean) THEN
            RAISE EXCEPTION 'VERIFICATION_FAILED'; END IF;
        IF p_kind='calendar' AND (d->>'calendar_hash'<>candidate->>'calendar_hash'
            OR jsonb_typeof(d->'sessions')<>'array' OR jsonb_array_length(d->'sessions')<20) THEN
            RAISE EXCEPTION 'APPROVED_CALENDAR_MISSING'; END IF;
    ELSIF p_kind='session' THEN
        PERFORM weekly.require_fields(d,ARRAY['data_kind','order_mutations','observations','market_data_hash','recording_hash',
            'exchange','session_id','opens_at','closes_at','first_observed_at','last_observed_at']);
        IF session_user<>'trady_shadow' OR c.state<>'SHADOW' OR d->>'data_kind'<>'REAL'
           OR (d->>'order_mutations')::integer<>0 OR (d->>'observations')::integer<2
           OR d->>'market_data_hash' !~ '^[a-f0-9]{64}$'
           OR d->>'recording_hash' !~ '^[a-f0-9]{64}$' THEN RAISE EXCEPTION 'REAL_SHADOW_SESSION_REQUIRED'; END IF;
        SELECT canonical::jsonb INTO STRICT calendar FROM release.evidence WHERE evidence_id=c.evidence->>'calendar'
            AND kind='calendar' AND subject_hash=c.subject_hash;
        SELECT v INTO STRICT interval FROM jsonb_array_elements(calendar->'sessions') v
            WHERE v->>'session_id'=d->>'session_id' AND v->>'exchange'=d->>'exchange';
        IF d->>'opens_at'<>interval->>'opens_at' OR d->>'closes_at'<>interval->>'closes_at'
           OR (d->>'first_observed_at')::timestamptz>(interval->>'opens_at')::timestamptz
           OR (d->>'last_observed_at')::timestamptz<(interval->>'closes_at')::timestamptz
           OR (interval->>'closes_at')::timestamptz>clock_timestamp()
           OR (interval->>'opens_at')::timestamptz<c.updated_at THEN RAISE EXCEPTION 'INCOMPLETE_OR_RETROSPECTIVE_SHADOW_SESSION'; END IF;
    ELSIF p_kind='owner' THEN
        PERFORM weekly.require_fields(d,ARRAY['owner_id','account_id','mode','policy_hash','decision','authority','valid_until']);
        IF session_user<>'trady_release_owner' OR c.state<>'SHADOW' OR d->>'owner_id'<>candidate->>'owner_id'
           OR d->>'account_id'<>candidate->>'account_id' OR d->>'mode'<>candidate->>'mode'
           OR d->>'policy_hash'<>candidate->>'policy_hash' OR d->>'decision'<>'APPROVE'
           OR d->>'authority'<>'OWNER_LIVE' OR (d->>'valid_until')::timestamptz<=clock_timestamp() THEN
            RAISE EXCEPTION 'SCOPED_OWNER_APPROVAL_REQUIRED'; END IF;
    ELSE RAISE EXCEPTION 'UNKNOWN_RELEASE_EVIDENCE_KIND'; END IF;
    INSERT INTO release.evidence VALUES(id,c.subject_hash,p_kind,p_canonical,h,session_user,clock_timestamp()) ON CONFLICT DO NOTHING;
    RETURN id;
END $$;

-- Executor may read only the current scope's ACTIVE authority, not propose,
-- attest, approve or advance a release. Every referenced content hash is checked.
CREATE FUNCTION release.current_active(p_account text,p_mode text) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog,weekly,release AS $$
DECLARE id text; resolved jsonb; d jsonb; approval jsonb;
BEGIN
    SELECT release_id INTO STRICT id FROM release.active WHERE account_id=p_account AND mode=p_mode;
    resolved:=release.resolve(id);d:=resolved->'candidate';
    approval:=resolved->'documents'->(resolved->'evidence'->>'owner')->'content';
    IF resolved->>'state'<>'ACTIVE' OR (resolved->>'is_current_active')::boolean IS NOT TRUE
       OR d->>'account_id'<>p_account OR d->>'mode'<>p_mode
       OR (d->>'effective_from')::timestamptz>clock_timestamp() OR (d->>'valid_until')::timestamptz<=clock_timestamp()
       OR approval IS NULL OR (approval->>'valid_until')::timestamptz<=clock_timestamp() THEN
        RAISE EXCEPTION 'ACTIVE_RELEASE_AUTHORITY_UNAVAILABLE'; END IF;
    RETURN resolved;
END $$;
CREATE FUNCTION release.resolve(p_id text) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog,weekly,release AS $$
DECLARE r release.candidates; item record; documents jsonb:='{}'::jsonb;
BEGIN
    SELECT * INTO STRICT r FROM release.candidates WHERE release_id=p_id;
    IF weekly.hash(r.canonical)<>r.subject_hash THEN RAISE EXCEPTION 'CANDIDATE_CONTENT_TAMPERED'; END IF;
    FOR item IN SELECT * FROM release.evidence WHERE subject_hash=r.subject_hash LOOP
        IF weekly.hash(item.canonical)<>item.content_hash THEN RAISE EXCEPTION 'EVIDENCE_CONTENT_TAMPERED'; END IF;
        documents:=documents||jsonb_build_object(item.evidence_id,jsonb_build_object('content',item.canonical::jsonb,
            'content_hash',item.content_hash,'producer',item.producer));
    END LOOP;
    RETURN jsonb_build_object('release_id',r.release_id,'subject_hash',r.subject_hash,'candidate',r.canonical::jsonb,
        'state',r.state,'revision',r.revision,'evidence',r.evidence,'documents',documents,
        'is_current_active',EXISTS(SELECT 1 FROM release.active WHERE release_id=p_id));
END $$;
CREATE FUNCTION release.advance(p_id text,p_expected bigint,p_next text,p_evidence jsonb) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog,weekly,release AS $$
DECLARE c release.candidates; d jsonb; e jsonb; n integer; sessions integer; owner_doc jsonb; readiness jsonb; active_id text;
BEGIN
    SELECT * INTO STRICT c FROM release.candidates WHERE release_id=p_id FOR UPDATE;
    PERFORM release.resolve(p_id); d:=c.canonical::jsonb; e:=c.evidence||p_evidence;
    IF c.revision<>p_expected THEN RAISE EXCEPTION 'RELEASE_REVISION_CONFLICT'; END IF;
    IF (c.state,p_next) NOT IN (('CANDIDATE','VERIFIED'),('VERIFIED','SHADOW'),('SHADOW','APPROVED'),('APPROVED','ACTIVE')) THEN
        RAISE EXCEPTION 'INVALID_RELEASE_TRANSITION'; END IF;
    IF p_next='VERIFIED' THEN
        SELECT count(*) INTO n FROM release.evidence WHERE evidence_id=e->>'verification' AND subject_hash=c.subject_hash
            AND kind='verification' AND producer='trady_verifier' AND (canonical::jsonb->>'passed')::boolean;
        IF n<>1 THEN RAISE EXCEPTION 'INDEPENDENT_VERIFICATION_MISSING'; END IF;
    ELSIF p_next='SHADOW' THEN
        SELECT count(*) INTO n FROM release.evidence WHERE evidence_id=e->>'calendar' AND subject_hash=c.subject_hash
            AND kind='calendar' AND producer='trady_verifier';
        IF n<>1 THEN RAISE EXCEPTION 'APPROVED_CALENDAR_MISSING'; END IF;
    ELSIF p_next='APPROVED' THEN
        SELECT count(*) INTO sessions FROM release.evidence WHERE subject_hash=c.subject_hash AND kind='session' AND producer='trady_shadow';
        SELECT canonical::jsonb INTO owner_doc FROM release.evidence WHERE evidence_id=e->>'owner'
            AND subject_hash=c.subject_hash AND kind='owner' AND producer='trady_release_owner';
        IF sessions<20 OR owner_doc IS NULL OR (owner_doc->>'valid_until')::timestamptz<=clock_timestamp()
           OR (d->>'mode'='live' AND owner_doc->>'authority'<>'OWNER_LIVE') THEN RAISE EXCEPTION 'TWENTY_REAL_SESSIONS_AND_OWNER_REQUIRED'; END IF;
    ELSIF p_next='ACTIVE' THEN
        SELECT canonical::jsonb INTO owner_doc FROM release.evidence WHERE evidence_id=e->>'owner' AND subject_hash=c.subject_hash;
        SELECT canonical::jsonb INTO readiness FROM release.evidence WHERE evidence_id=e->>'readiness' AND subject_hash=c.subject_hash
            AND kind='readiness' AND producer='trady_verifier';
        IF owner_doc IS NULL OR (owner_doc->>'valid_until')::timestamptz<=clock_timestamp() OR readiness IS NULL
           OR NOT (readiness->>'ready')::boolean OR (readiness->>'valid_until')::timestamptz<=clock_timestamp()
           OR readiness->>'account_id'<>d->>'account_id' OR readiness->>'mode'<>d->>'mode'
           OR (owner_doc->>'recorded_at')::timestamptz>(d->>'effective_from')::timestamptz
           OR (d->>'effective_from')::timestamptz>clock_timestamp() OR (d->>'valid_until')::timestamptz<=clock_timestamp() THEN
            RAISE EXCEPTION 'FRESH_SCOPED_READINESS_AND_APPROVAL_REQUIRED'; END IF;
        PERFORM pg_advisory_xact_lock(hashtextextended(d->>'account_id'||d->>'mode',6006));
        SELECT release_id INTO active_id FROM release.active WHERE account_id=d->>'account_id' AND mode=d->>'mode' FOR UPDATE;
        IF active_id IS DISTINCT FROM d->>'supersedes_release_id' THEN RAISE EXCEPTION 'ACTIVE_RELEASE_CHANGED'; END IF;
        INSERT INTO release.active VALUES(d->>'account_id',d->>'mode',p_id)
            ON CONFLICT(account_id,mode) DO UPDATE SET release_id=excluded.release_id;
    END IF;
    -- Existing gate references cannot be replaced while advancing.
    IF EXISTS(SELECT 1 FROM jsonb_each(c.evidence) old WHERE p_evidence ? old.key AND p_evidence->old.key<>old.value) THEN
        RAISE EXCEPTION 'RELEASE_EVIDENCE_REBOUND'; END IF;
    UPDATE release.candidates SET state=p_next,evidence=e,revision=revision+1,updated_at=clock_timestamp() WHERE release_id=p_id;
    INSERT INTO release.events(release_id,revision,state,evidence,actor) VALUES(p_id,p_expected+1,p_next,e,session_user);
    RETURN release.resolve(p_id);
END $$;

REVOKE ALL ON ALL TABLES IN SCHEMA weekly,release FROM PUBLIC;
REVOKE ALL ON ALL FUNCTIONS IN SCHEMA weekly,release FROM PUBLIC;
GRANT USAGE ON SCHEMA weekly TO trady_lab,trady_verifier,trady_shadow,trady_analyst;
GRANT EXECUTE ON FUNCTION weekly.freeze(text,jsonb,text,integer),weekly.load_run(text,bigint),
    weekly.renew(text,bigint,integer),weekly.release_run(text,bigint),weekly.reserve_trial(text,bigint,jsonb,integer),
    weekly.checkpoint(text,bigint,jsonb),weekly.claim_holdout(text,bigint,text,text),weekly.finish_holdout(text,bigint,jsonb)
    TO trady_lab;
GRANT EXECUTE ON FUNCTION weekly.record_ledger(text,text,text,text,text) TO trady_verifier,trady_shadow,trady_analyst;
GRANT EXECUTE ON FUNCTION weekly.export_ledger(text,text,timestamptz) TO trady_lab,trady_verifier;
GRANT USAGE ON SCHEMA release TO trady_lab,trady_verifier,trady_shadow,trady_releaser,trady_release_owner;
GRANT EXECUTE ON FUNCTION release.candidate(text) TO trady_lab;
GRANT EXECUTE ON FUNCTION release.record_evidence(text,text) TO trady_verifier,trady_shadow,trady_release_owner;
GRANT EXECUTE ON FUNCTION release.resolve(text) TO trady_lab,trady_verifier,trady_shadow,trady_releaser,trady_release_owner;
GRANT EXECUTE ON FUNCTION release.advance(text,bigint,text,jsonb) TO trady_releaser;
GRANT USAGE ON SCHEMA release TO trady_executor;
GRANT EXECUTE ON FUNCTION release.current_active(text,text) TO trady_executor;
