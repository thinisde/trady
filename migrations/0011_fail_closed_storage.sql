-- Forward-only hardening. Migration 6 remains source-pinned for existing installs.
-- Private legacy implementations retain their transactions; validated entry points
-- below are the only functions granted to service roles.

-- Advisory/row locks cannot refresh a transaction snapshot or lock absent rows.
-- These boundaries rely on READ COMMITTED's fresh snapshots after lock waits;
-- reject fixed-snapshot isolation before publishing content or resolving authority.
CREATE FUNCTION research.require_read_committed() RETURNS void
LANGUAGE plpgsql SET search_path=pg_catalog,pg_temp AS $$
BEGIN
    IF current_setting('transaction_isolation') IS DISTINCT FROM 'read committed' THEN
        RAISE EXCEPTION 'STORAGE_REQUIRES_READ_COMMITTED' USING ERRCODE='0A000';
    END IF;
END $$;
REVOKE ALL ON FUNCTION research.require_read_committed() FROM PUBLIC;
-- The immutable-erasure BEFORE trigger runs with its analyst caller's privileges.
GRANT EXECUTE ON FUNCTION research.require_read_committed() TO trady_analyst;

CREATE OR REPLACE FUNCTION weekly.assert_fence(p_id text,p_generation bigint) RETURNS weekly.runs
LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog,pg_temp AS $$
DECLARE r weekly.runs;
BEGIN
    SELECT * INTO STRICT r FROM weekly.runs WHERE run_id=p_id FOR UPDATE;
    IF p_generation IS NULL OR p_generation<1 OR r.backend IS DISTINCT FROM pg_backend_pid()
       OR r.generation IS DISTINCT FROM p_generation OR (r.lease_until>clock_timestamp()) IS NOT TRUE THEN
        RAISE EXCEPTION 'WEEKLY_LEASE_LOST';
    END IF;
    RETURN r;
END $$;

ALTER FUNCTION weekly.freeze(text,jsonb,text,integer) RENAME TO freeze_v6;
REVOKE ALL ON FUNCTION weekly.freeze_v6(text,jsonb,text,integer) FROM PUBLIC,trady_lab;
CREATE FUNCTION weekly.freeze(p_id text,p_state jsonb,p_worker text,p_ttl integer) RETURNS bigint
LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog,pg_temp AS $$
BEGIN
    IF p_id IS NULL OR p_worker IS NULL OR p_ttl IS NULL THEN RAISE EXCEPTION 'INVALID_WEEKLY_FREEZE'; END IF;
    PERFORM weekly.require_fields(p_state,ARRAY['manifest','plan','policy','trials','started_at']);
    IF (jsonb_typeof(p_state->'policy')='object' AND jsonb_typeof(p_state->'trials')='array'
        AND jsonb_typeof(p_state->'plan'->'candidates')='array') IS NOT TRUE THEN
        RAISE EXCEPTION 'INVALID_WEEKLY_FREEZE'; END IF;
    RETURN weekly.freeze_v6(p_id,p_state,p_worker,p_ttl);
END $$;
GRANT EXECUTE ON FUNCTION weekly.freeze(text,jsonb,text,integer) TO trady_lab;

CREATE OR REPLACE FUNCTION weekly.renew(p_id text,p_generation bigint,p_ttl integer) RETURNS void
LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog,pg_temp AS $$
BEGIN
    PERFORM weekly.assert_fence(p_id,p_generation);
    IF (p_ttl BETWEEN 1 AND 3600) IS NOT TRUE THEN RAISE EXCEPTION 'INVALID_LEASE_TTL'; END IF;
    UPDATE weekly.runs SET lease_until=clock_timestamp()+make_interval(secs=>p_ttl) WHERE run_id=p_id;
END $$;

ALTER FUNCTION weekly.reserve_trial(text,bigint,jsonb,integer) RENAME TO reserve_trial_v6;
REVOKE ALL ON FUNCTION weekly.reserve_trial_v6(text,bigint,jsonb,integer) FROM PUBLIC,trady_lab;
CREATE FUNCTION weekly.reserve_trial(p_id text,p_generation bigint,p_trial jsonb,p_limit integer) RETURNS void
LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog,pg_temp AS $$
BEGIN
    IF (p_limit BETWEEN 1 AND 10000) IS NOT TRUE THEN RAISE EXCEPTION 'INVALID_TRIAL_RESERVATION'; END IF;
    PERFORM weekly.require_fields(p_trial,ARRAY['candidate_id','parameters','registered_at']);
    IF (p_trial->>'candidate_id' ~ '^[A-Za-z0-9_.:-]{1,160}$'
        AND jsonb_typeof(p_trial->'parameters')='object') IS NOT TRUE THEN RAISE EXCEPTION 'INVALID_TRIAL_RESERVATION'; END IF;
    PERFORM weekly.reserve_trial_v6(p_id,p_generation,p_trial,p_limit);
END $$;
GRANT EXECUTE ON FUNCTION weekly.reserve_trial(text,bigint,jsonb,integer) TO trady_lab;

ALTER FUNCTION weekly.checkpoint(text,bigint,jsonb) RENAME TO checkpoint_v6;
REVOKE ALL ON FUNCTION weekly.checkpoint_v6(text,bigint,jsonb) FROM PUBLIC,trady_lab;
CREATE FUNCTION weekly.checkpoint(p_id text,p_generation bigint,p_state jsonb) RETURNS void
LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog,pg_temp AS $$
BEGIN
    IF (jsonb_typeof(p_state->'trials')='array') IS NOT TRUE THEN RAISE EXCEPTION 'INVALID_WEEKLY_CHECKPOINT'; END IF;
    PERFORM weekly.checkpoint_v6(p_id,p_generation,p_state);
END $$;
GRANT EXECUTE ON FUNCTION weekly.checkpoint(text,bigint,jsonb) TO trady_lab;

ALTER FUNCTION weekly.claim_holdout(text,bigint,text,text) RENAME TO claim_holdout_v6;
REVOKE ALL ON FUNCTION weekly.claim_holdout_v6(text,bigint,text,text) FROM PUBLIC,trady_lab;
CREATE FUNCTION weekly.claim_holdout(p_id text,p_generation bigint,p_candidate text,p_hash text) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog,pg_temp AS $$
BEGIN
    IF (p_candidate ~ '^[A-Za-z0-9_.:-]{1,160}$' AND p_hash ~ '^[a-f0-9]{64}$') IS NOT TRUE THEN
        RAISE EXCEPTION 'HOLDOUT_REQUIRES_FROZEN_VERIFIED_CANDIDATE'; END IF;
    RETURN weekly.claim_holdout_v6(p_id,p_generation,p_candidate,p_hash);
END $$;
GRANT EXECUTE ON FUNCTION weekly.claim_holdout(text,bigint,text,text) TO trady_lab;

CREATE OR REPLACE FUNCTION weekly.finish_holdout(p_id text,p_generation bigint,p_report jsonb) RETURNS void
LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog,pg_temp AS $$
DECLARE h weekly.holdouts;
BEGIN
    PERFORM weekly.assert_fence(p_id,p_generation);
    PERFORM weekly.require_fields(p_report,ARRAY['report','run_id','dataset_hash','plan_hash','outer_intervals',
        'closed_trades','completed_shadow_weeks','shadow_closed_trades','cost_stress_multiple','improvement_lower_bound',
        'dependence_and_search_adjusted','causal_and_uncontaminated','risk_not_deteriorated','economic_feasibility_passed',
        'neighborhood_checks_passed']);
    PERFORM weekly.require_fields(p_report->'report',ARRAY['report_id','tool_id','input_hash','output_hash','exit_status','duration_ms','recorded_at']);
    SELECT * INTO STRICT h FROM weekly.holdouts WHERE run_id=p_id FOR UPDATE;
    IF p_report->'report'->>'input_hash' IS DISTINCT FROM h.input_hash OR p_report->>'run_id' IS DISTINCT FROM p_id
       OR p_report->>'dataset_hash' IS DISTINCT FROM h.dataset_hash
       OR (h.report IS NOT NULL AND h.report IS DISTINCT FROM p_report) THEN RAISE EXCEPTION 'HOLDOUT_REPORT_REBOUND'; END IF;
    UPDATE weekly.holdouts SET report=p_report WHERE run_id=p_id;
END $$;

ALTER FUNCTION weekly.record_ledger(text,text,text,text,text) RENAME TO record_ledger_v6;
REVOKE ALL ON FUNCTION weekly.record_ledger_v6(text,text,text,text,text) FROM PUBLIC,trady_verifier,trady_shadow,trady_analyst;
CREATE FUNCTION weekly.record_ledger(p_kind text,p_account text,p_mode text,p_id text,p_canonical text) RETURNS text
LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog,pg_temp AS $$
DECLARE d jsonb:=p_canonical::jsonb;
BEGIN
    IF p_kind IS NULL OR p_account IS NULL OR p_mode IS NULL OR p_id IS NULL OR p_canonical IS NULL THEN
        RAISE EXCEPTION 'INVALID_LEDGER_SCOPE'; END IF;
    PERFORM weekly.require_fields(d,ARRAY[]::text[]);
    IF p_kind='forecast' THEN
        PERFORM weekly.require_fields(d,ARRAY['metadata']);
        PERFORM weekly.require_fields(d->'metadata',ARRAY['prediction_id','generated_at','horizon']);
        PERFORM weekly.require_fields(d->'metadata'->'horizon',ARRAY['deadline_at']);
        IF NOT isfinite((d->'metadata'->>'generated_at')::timestamptz)
           OR NOT isfinite((d->'metadata'->'horizon'->>'deadline_at')::timestamptz) THEN RAISE EXCEPTION 'FORECAST_NOT_PROSPECTIVE'; END IF;
    ELSIF p_kind='label' THEN
        PERFORM weekly.require_fields(d,ARRAY['forecast_hash','source_hash','label_version','observed_through']);
        IF NOT isfinite((d->>'observed_through')::timestamptz) OR (d->>'source_hash' ~ '^[a-f0-9]{64}$') IS NOT TRUE THEN
            RAISE EXCEPTION 'UNMATURED_OR_MISBOUND_LABEL'; END IF;
    ELSIF p_kind='reconciliation' THEN
        PERFORM weekly.require_fields(d,ARRAY['account_id','mode','export_id','cutoff_at','source_hashes','report','equity','unresolved_items']);
        IF (isfinite((d->>'cutoff_at')::timestamptz) AND (d->>'cutoff_at')::timestamptz<=clock_timestamp()
            AND d ?& ARRAY['open_holdings','pending_orders']
            AND jsonb_typeof(d->'source_hashes')='array' AND jsonb_array_length(d->'source_hashes')>0) IS NOT TRUE THEN
            RAISE EXCEPTION 'INDEPENDENT_RECONCILIATION_REQUIRED'; END IF;
    END IF;
    RETURN weekly.record_ledger_v6(p_kind,p_account,p_mode,p_id,p_canonical);
END $$;
GRANT EXECUTE ON FUNCTION weekly.record_ledger(text,text,text,text,text) TO trady_verifier,trady_shadow,trady_analyst;

ALTER FUNCTION release.candidate(text) RENAME TO candidate_v6;
REVOKE ALL ON FUNCTION release.candidate_v6(text) FROM PUBLIC,trady_lab;
CREATE FUNCTION release.candidate(p_canonical text) RETURNS text
LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog,pg_temp AS $$
DECLARE d jsonb:=p_canonical::jsonb;
BEGIN
    PERFORM weekly.require_fields(d,ARRAY['effective_from','valid_until','model_hashes','parameters','envelope','revision_id','owner_id']);
    IF (isfinite((d->>'effective_from')::timestamptz) AND isfinite((d->>'valid_until')::timestamptz)
        AND jsonb_typeof(d->'model_hashes')='object' AND jsonb_typeof(d->'parameters')='object'
        AND jsonb_typeof(d->'envelope')='object' AND d->>'revision_id' ~ '^[A-Za-z0-9_.:-]{1,160}$'
        AND d->>'owner_id' ~ '^[A-Za-z0-9_.:-]{1,160}$') IS NOT TRUE THEN RAISE EXCEPTION 'INVALID_RELEASE_CANDIDATE'; END IF;
    IF EXISTS(SELECT 1 FROM jsonb_each(d->'model_hashes') e WHERE (e.key ~ '^[A-Za-z0-9_.:-]{1,160}$'
        AND jsonb_typeof(e.value)='string' AND e.value#>>'{}' ~ '^[a-f0-9]{64}$') IS NOT TRUE) THEN
        RAISE EXCEPTION 'INVALID_RELEASE_CANDIDATE'; END IF;
    RETURN release.candidate_v6(p_canonical);
END $$;
GRANT EXECUTE ON FUNCTION release.candidate(text) TO trady_lab;

ALTER FUNCTION release.record_evidence(text,text) RENAME TO record_evidence_v6;
REVOKE ALL ON FUNCTION release.record_evidence_v6(text,text) FROM PUBLIC,trady_verifier,trady_shadow,trady_release_owner;
CREATE FUNCTION release.record_evidence(p_kind text,p_canonical text) RETURNS text
LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog,pg_temp AS $$
DECLARE d jsonb:=p_canonical::jsonb; item jsonb; candidate jsonb;
BEGIN
    IF (p_kind IN ('verification','calendar','session','owner','readiness')) IS NOT TRUE THEN RAISE EXCEPTION 'UNKNOWN_RELEASE_EVIDENCE_KIND'; END IF;
    PERFORM weekly.require_fields(d,ARRAY['subject_hash','scope_hash','kind','recorded_at']);
    IF NOT isfinite((d->>'recorded_at')::timestamptz) THEN RAISE EXCEPTION 'MISBOUND_RELEASE_EVIDENCE'; END IF;
    IF p_kind='calendar' THEN
        PERFORM weekly.require_fields(d,ARRAY['sessions','sessions_canonical','calendar_hash']);
        IF (jsonb_typeof(d->'sessions')='array') IS NOT TRUE THEN RAISE EXCEPTION 'APPROVED_CALENDAR_MISSING'; END IF;
        FOR item IN SELECT value FROM jsonb_array_elements(d->'sessions') LOOP
            PERFORM weekly.require_fields(item,ARRAY['session_id','exchange','opens_at','closes_at']);
            IF (item->>'session_id' ~ '^[A-Za-z0-9_.:-]{1,160}$' AND item->>'exchange' ~ '^[A-Za-z0-9_.:-]{1,160}$'
                AND isfinite((item->>'opens_at')::timestamptz) AND isfinite((item->>'closes_at')::timestamptz)) IS NOT TRUE THEN
                RAISE EXCEPTION 'INVALID_OR_DUPLICATE_CALENDAR_SESSION'; END IF;
        END LOOP;
    ELSIF p_kind='readiness' THEN
        PERFORM weekly.require_fields(d,ARRAY['ready','valid_until','account_id','mode','snapshot_hash','capabilities_hash']);
        SELECT canonical::jsonb INTO STRICT candidate FROM release.candidates WHERE subject_hash=d->>'subject_hash';
        IF (jsonb_typeof(d->'ready')='boolean' AND isfinite((d->>'valid_until')::timestamptz)
            AND (d->>'valid_until')::timestamptz>clock_timestamp()
            AND (d->>'valid_until')::timestamptz<=(d->>'recorded_at')::timestamptz+interval '5 minutes'
            AND d->>'account_id'=candidate->>'account_id' AND d->>'mode'=candidate->>'mode') IS NOT TRUE THEN
            RAISE EXCEPTION 'INVALID_READINESS_EVIDENCE'; END IF;
    ELSIF p_kind='owner' THEN
        PERFORM weekly.require_fields(d,ARRAY['valid_until']);
        IF NOT isfinite((d->>'valid_until')::timestamptz) THEN RAISE EXCEPTION 'SCOPED_OWNER_APPROVAL_REQUIRED'; END IF;
    ELSIF p_kind='session' THEN
        PERFORM weekly.require_fields(d,ARRAY['opens_at','closes_at','first_observed_at','last_observed_at']);
        IF NOT isfinite((d->>'opens_at')::timestamptz) OR NOT isfinite((d->>'closes_at')::timestamptz)
           OR NOT isfinite((d->>'first_observed_at')::timestamptz) OR NOT isfinite((d->>'last_observed_at')::timestamptz) THEN
            RAISE EXCEPTION 'REAL_SHADOW_SESSION_REQUIRED'; END IF;
    ELSIF p_kind='verification' THEN
        PERFORM weekly.require_fields(d,ARRAY['passed','checks']);
        IF jsonb_typeof(d->'passed') IS DISTINCT FROM 'boolean' OR jsonb_typeof(d->'checks') IS DISTINCT FROM 'object' THEN
            RAISE EXCEPTION 'VERIFICATION_FAILED'; END IF;
    END IF;
    RETURN release.record_evidence_v6(p_kind,p_canonical);
END $$;
GRANT EXECUTE ON FUNCTION release.record_evidence(text,text) TO trady_verifier,trady_shadow,trady_release_owner;

ALTER FUNCTION release.advance(text,bigint,text,jsonb) RENAME TO advance_v6;
REVOKE ALL ON FUNCTION release.advance_v6(text,bigint,text,jsonb) FROM PUBLIC,trady_releaser;
CREATE FUNCTION release.advance(p_id text,p_expected bigint,p_next text,p_evidence jsonb) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog,pg_temp AS $$
BEGIN
    PERFORM research.require_read_committed();
    IF p_id IS NULL OR (p_expected>=0) IS NOT TRUE OR (p_next IN ('VERIFIED','SHADOW','APPROVED','ACTIVE')) IS NOT TRUE
       OR jsonb_typeof(p_evidence) IS DISTINCT FROM 'object' THEN RAISE EXCEPTION 'INVALID_RELEASE_TRANSITION'; END IF;
    IF EXISTS(SELECT 1 FROM jsonb_each(p_evidence) e WHERE e.key NOT IN ('verification','calendar','owner','readiness')
        OR (jsonb_typeof(e.value)='string' AND e.value#>>'{}' ~ '^evidence_[a-f0-9]{64}$') IS NOT TRUE) THEN
        RAISE EXCEPTION 'INVALID_RELEASE_EVIDENCE_REFERENCE'; END IF;
    RETURN release.advance_v6(p_id,p_expected,p_next,p_evidence);
END $$;
GRANT EXECUTE ON FUNCTION release.advance(text,bigint,text,jsonb) TO trady_releaser;

CREATE OR REPLACE FUNCTION release.current_active(p_account text,p_mode text) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog,pg_temp AS $$
DECLARE id text; resolved jsonb; d jsonb; approval jsonb; readiness jsonb; readiness_id text; now_at timestamptz:=clock_timestamp();
BEGIN
    PERFORM research.require_read_committed();
    SELECT release_id INTO STRICT id FROM release.active WHERE account_id=p_account AND mode=p_mode;
    resolved:=release.resolve(id); d:=resolved->'candidate';
    approval:=resolved->'documents'->(resolved->'evidence'->>'owner')->'content';
    -- Readiness is a renewable verifier attestation. Its newest record wins even
    -- when it says not ready; never fall back to an older successful snapshot.
    SELECT evidence_id INTO readiness_id FROM release.evidence WHERE subject_hash=resolved->>'subject_hash'
        AND kind='readiness' AND producer='trady_verifier' ORDER BY recorded_at DESC,evidence_id DESC LIMIT 1;
    IF readiness_id IS NOT NULL THEN resolved:=jsonb_set(resolved,'{evidence,readiness}',to_jsonb(readiness_id)); END IF;
    readiness:=resolved->'documents'->(resolved->'evidence'->>'readiness')->'content';
    IF (resolved->>'state'='ACTIVE' AND resolved->'is_current_active'='true'::jsonb
        AND d->>'account_id'=p_account AND d->>'mode'=p_mode
        AND isfinite((d->>'effective_from')::timestamptz) AND isfinite((d->>'valid_until')::timestamptz)
        AND (d->>'effective_from')::timestamptz<=now_at AND (d->>'valid_until')::timestamptz>now_at
        AND resolved->'documents'->(resolved->'evidence'->>'owner')->>'producer'='trady_release_owner'
        AND approval->>'kind'='owner' AND approval->>'subject_hash'=resolved->>'subject_hash'
        AND approval->>'owner_id'=d->>'owner_id' AND approval->>'account_id'=p_account AND approval->>'mode'=p_mode
        AND approval->>'policy_hash'=d->>'policy_hash' AND approval->>'decision'='APPROVE' AND approval->>'authority'='OWNER_LIVE'
        AND isfinite((approval->>'valid_until')::timestamptz) AND (approval->>'valid_until')::timestamptz>now_at
        AND isfinite((approval->>'recorded_at')::timestamptz)
        AND (approval->>'recorded_at')::timestamptz<=(d->>'effective_from')::timestamptz
        AND resolved->'documents'->(resolved->'evidence'->>'readiness')->>'producer'='trady_verifier'
        AND readiness->>'kind'='readiness' AND readiness->>'subject_hash'=resolved->>'subject_hash'
        AND readiness->'ready'='true'::jsonb AND readiness->>'account_id'=p_account AND readiness->>'mode'=p_mode
        AND readiness->>'snapshot_hash' ~ '^[a-f0-9]{64}$' AND readiness->>'capabilities_hash' ~ '^[a-f0-9]{64}$'
        AND (readiness->>'recorded_at')::timestamptz<=now_at AND isfinite((readiness->>'valid_until')::timestamptz)
        AND (readiness->>'valid_until')::timestamptz>now_at
        AND (readiness->>'valid_until')::timestamptz<=(readiness->>'recorded_at')::timestamptz+interval '5 minutes') IS NOT TRUE THEN
        RAISE EXCEPTION 'ACTIVE_RELEASE_AUTHORITY_UNAVAILABLE'; END IF;
    RETURN resolved;
END $$;
GRANT EXECUTE ON FUNCTION release.current_active(text,text) TO trady_lab,trady_verifier,trady_shadow,trady_releaser,trady_release_owner;

-- All affected definers resolve only owner-controlled schemas, with pg_temp last.
DO $$ DECLARE f record; BEGIN
    FOR f IN SELECT p.oid::regprocedure AS signature FROM pg_proc p JOIN pg_namespace n ON n.oid=p.pronamespace
        WHERE n.nspname IN ('weekly','release') AND p.prosecdef LOOP
        EXECUTE format('ALTER FUNCTION %s SET search_path=pg_catalog,weekly,release,pg_temp',f.signature);
    END LOOP;
END $$;
-- Newly created wrappers must not inherit PostgreSQL's default PUBLIC execute.
REVOKE ALL ON ALL FUNCTIONS IN SCHEMA weekly,release FROM PUBLIC;

-- Legacy cap reductions could leave usage above the stored ceiling. Preserve
-- every charged credit and leave those windows exhausted instead of refunding.
UPDATE research.provider_budget_windows SET credit_limit=used_credits WHERE used_credits>credit_limit;
ALTER TABLE research.provider_budget_windows ADD CONSTRAINT provider_credits_within_limit CHECK (used_credits<=credit_limit);
ALTER TABLE research.financial_collections ADD CONSTRAINT financial_collection_status CHECK (status IN ('OK','PARTIAL','STALE','ERROR','DENIED','UNAVAILABLE'));
CREATE FUNCTION research.guard_provider_credit_window() RETURNS trigger
LANGUAGE plpgsql SET search_path=pg_catalog,pg_temp AS $$
BEGIN
    IF (NEW.budget_key,NEW.window_kind,NEW.window_start) IS DISTINCT FROM (OLD.budget_key,OLD.window_kind,OLD.window_start)
        OR NEW.used_credits<OLD.used_credits OR NEW.credit_limit>OLD.credit_limit THEN
        RAISE EXCEPTION 'PROVIDER_CREDIT_WINDOW_CANNOT_REBIND_REFUND_OR_EXPAND'; END IF;
    RETURN NEW;
END $$;
CREATE TRIGGER provider_credit_window_monotonic BEFORE UPDATE ON research.provider_budget_windows
FOR EACH ROW EXECUTE FUNCTION research.guard_provider_credit_window();
REVOKE ALL ON FUNCTION research.guard_provider_credit_window() FROM PUBLIC;

-- The current reviewed configuration is distinct from immutable registry history.
CREATE TABLE research.intelligence_scopes (
    owner_id text NOT NULL CHECK (owner_id ~ '^[A-Za-z0-9_-]{1,96}$'),
    account_id text NOT NULL CHECK (account_id ~ '^[A-Za-z0-9_-]{1,96}$'),
    registry_hash text NOT NULL REFERENCES research.intelligence_registries,
    PRIMARY KEY(owner_id,account_id)
);
REVOKE ALL ON research.intelligence_scopes FROM PUBLIC;
GRANT SELECT ON research.intelligence_scopes TO trady_analyst;
CREATE TABLE research.intelligence_registry_retirements (
    registry_hash text PRIMARY KEY REFERENCES research.intelligence_registries,
    replacement_hash text NOT NULL REFERENCES research.intelligence_registries,
    retired_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    CHECK (registry_hash<>replacement_hash)
);
REVOKE ALL ON research.intelligence_registry_retirements FROM PUBLIC;
GRANT SELECT ON research.intelligence_registry_retirements TO trady_analyst;
-- Preserve compact typed serialization for database hash checks. Historical rows
-- predate this column; every new insert must supply it and erasure clears it too.
ALTER TABLE research.intelligence_observations ADD COLUMN canonical text;

CREATE FUNCTION research.intelligence_keys(p_document jsonb,p_allowed text[]) RETURNS boolean
LANGUAGE sql IMMUTABLE SET search_path=pg_catalog,pg_temp AS $$
    SELECT coalesce(jsonb_typeof(p_document)='object' AND p_document-p_allowed='{}'::jsonb,false)
$$;
CREATE FUNCTION research.intelligence_no_tax(p_document jsonb) RETURNS boolean
LANGUAGE plpgsql IMMUTABLE SET search_path=pg_catalog,pg_temp AS $$
DECLARE v jsonb;
BEGIN
    IF jsonb_typeof(p_document)='object' THEN
        IF p_document->>'feature'='TAX' THEN RETURN false; END IF;
        FOR v IN SELECT value FROM jsonb_each(p_document) LOOP
            IF NOT research.intelligence_no_tax(v) THEN RETURN false; END IF;
        END LOOP;
    ELSIF jsonb_typeof(p_document)='array' THEN
        FOR v IN SELECT value FROM jsonb_array_elements(p_document) LOOP
            IF NOT research.intelligence_no_tax(v) THEN RETURN false; END IF;
        END LOOP;
    END IF;
    RETURN true;
END $$;
CREATE FUNCTION research.intelligence_feature(p_feature text) RETURNS boolean
LANGUAGE sql IMMUTABLE SET search_path=pg_catalog,pg_temp AS $$
    SELECT coalesce(p_feature IN ('OPTIONS','COORDINATION','EVENT','SHORT_INTEREST','INSIDER','CROSS_ASSET','ATTENTION','ACCOUNTING','MICROSTRUCTURE'),false)
$$;
CREATE FUNCTION research.intelligence_ids(p_ids jsonb) RETURNS boolean
LANGUAGE plpgsql IMMUTABLE SET search_path=pg_catalog,pg_temp AS $$
BEGIN
    IF jsonb_typeof(p_ids) IS DISTINCT FROM 'array' OR jsonb_array_length(p_ids)=0 THEN RETURN false; END IF;
    RETURN NOT EXISTS(SELECT 1 FROM jsonb_array_elements(p_ids) v WHERE
        (jsonb_typeof(v)='string' AND v#>>'{}' ~ '^[A-Za-z0-9_-]{1,96}$') IS NOT TRUE);
END $$;

CREATE FUNCTION research.intelligence_input_keys(p_feature text,p_input jsonb) RETURNS boolean
LANGUAGE sql IMMUTABLE SET search_path=pg_catalog,pg_temp AS $$
    SELECT research.intelligence_keys(p_input,fields) AND p_input ?& fields AND research.intelligence_feature(p_feature)
    FROM (SELECT CASE p_feature
        WHEN 'OPTIONS' THEN ARRAY['instrument_id','currency','venue','underlying','assumptions','contracts','horizon_seconds','max_quote_age_seconds','max_alignment_seconds','max_relative_spread_fraction','atm_max_moneyness_fraction','minimum_open_interest','skew_absolute_delta','skew_delta_tolerance','history_start','iv_history','sector_comparables','realized_volatility','event']
        WHEN 'COORDINATION' THEN ARRAY['issuer_id','claim_cluster','window_start','window_end','expires_at','source_window_key','event_category','events','coverage','invalidated_origin_ids','market_deviation','market_deviation_unavailable_reason','thresholds']
        WHEN 'EVENT' THEN ARRAY['event_id','issuer_id','event_type','metric','period','unit','release_at','clock_verified_at','clock_valid_until','frozen_estimate_id','expectation_frozen_at','drift_baseline_id','expectations','actuals','normalization','reaction','pricing_proxy','confounders']
        WHEN 'SHORT_INTEREST' THEN ARRAY['issuer_id','adjustment_basis','reports','float','average_volume','borrow','borrow_history','market_context','max_report_age_seconds','max_context_age_seconds','history_start','thresholds']
        WHEN 'INSIDER' THEN ARRAY['issuer_cik','window_start','window_end','reporting_currency','minimum_cluster_value','minimum_distinct_buyers','coverage','coverage_reason','filings']
        WHEN 'CROSS_ASSET' THEN ARRAY['region','effective_horizon_seconds','series','pairs','flows','stress','breadth']
        WHEN 'ATTENTION' THEN ARRAY['issuer_id','series','verified_events','coordination_warning_reference','thresholds']
        WHEN 'ACCOUNTING' THEN ARRAY['issuer_id','current_period','comparison_period','crosswalk_revision','accounting_basis','consolidated_scope','cohort','minimum_denominator','minimum_cohort_size','facts','comparability_issues','peers']
        WHEN 'MICROSTRUCTURE' THEN ARRAY['instrument_id','venue','currency','tick_size','size_increment','max_age_milliseconds','max_clock_skew_milliseconds','depth_limit','capacity_band_ticks','routing','book','history','auction','markout']
        ELSE ARRAY[]::text[] END AS fields) required
$$;

CREATE FUNCTION research.intelligence_payload_bound(p_value jsonb,p_evidence jsonb,p_source text DEFAULT NULL) RETURNS boolean
LANGUAGE plpgsql IMMUTABLE SET search_path=pg_catalog,pg_temp AS $$
DECLARE item record; value jsonb; source text:=p_source; reference text;
BEGIN
    IF jsonb_typeof(p_value)='object' THEN
        source:=coalesce(p_value->>'source_id',source);
        IF source IS NOT NULL AND NOT EXISTS(SELECT 1 FROM jsonb_array_elements(p_evidence) e WHERE e->>'source_id'=source) THEN RETURN false; END IF;
        FOR item IN SELECT * FROM jsonb_each(p_value) LOOP
            IF (item.key='reference' OR item.key ~ '(_ref|_reference)$') AND jsonb_typeof(item.value)='string' THEN
                reference:=item.value#>>'{}';
                IF NOT EXISTS(SELECT 1 FROM jsonb_array_elements(p_evidence) e WHERE (source IS NULL OR e->>'source_id'=source)
                    AND (reference=e->>'source_reference' OR starts_with(reference,(e->>'source_reference')||'#'))) THEN RETURN false; END IF;
            END IF;
            IF (item.key='references' OR item.key ~ '(_refs|_references)$') AND jsonb_typeof(item.value)='array' THEN
                FOR value IN SELECT v FROM jsonb_array_elements(item.value) v WHERE jsonb_typeof(v)='string' LOOP
                    reference:=value#>>'{}';
                    IF NOT EXISTS(SELECT 1 FROM jsonb_array_elements(p_evidence) e WHERE (source IS NULL OR e->>'source_id'=source)
                        AND (reference=e->>'source_reference' OR starts_with(reference,(e->>'source_reference')||'#'))) THEN RETURN false; END IF;
                END LOOP;
            END IF;
            IF NOT research.intelligence_payload_bound(item.value,p_evidence,source) THEN RETURN false; END IF;
        END LOOP;
    ELSIF jsonb_typeof(p_value)='array' THEN
        FOR value IN SELECT v FROM jsonb_array_elements(p_value) v LOOP
            IF NOT research.intelligence_payload_bound(value,p_evidence,source) THEN RETURN false; END IF;
        END LOOP;
    END IF;
    RETURN true;
END $$;

-- Return NULL for unavailable rights. No caller timestamp may extend retention.
CREATE FUNCTION research.intelligence_retention(p_document jsonb,p_config jsonb,p_derived boolean) RETURNS timestamptz
LANGUAGE plpgsql STABLE SET search_path=pg_catalog,pg_temp AS $$
DECLARE e jsonb; s jsonb; deadline timestamptz:='infinity'; candidate timestamptz; now_at timestamptz:=statement_timestamp();
BEGIN
    IF (p_document->'scope'=p_config->'scope' AND p_config->'enabled' ? (p_document->'payload'->>'feature')
        AND jsonb_typeof(p_document->'evidence')='array' AND jsonb_array_length(p_document->'evidence') BETWEEN 1 AND 64) IS NOT TRUE THEN RETURN NULL; END IF;
    FOR e IN SELECT value FROM jsonb_array_elements(p_document->'evidence') LOOP
        SELECT value INTO s FROM jsonb_array_elements(p_config->'sources') WHERE value->>'source_id'=e->>'source_id';
        IF (s->'enabled'='true'::jsonb AND s->'audit_markers'='true'::jsonb
            AND s->'rights'->'normalized_storage'='true'::jsonb AND (NOT p_derived OR s->'derived_storage'='true'::jsonb)
            AND s->>'provider'=e->>'provider' AND s->>'rights_revision'=e->>'rights_revision'
            AND s->'entity_ids' ? (p_document->>'entity_id') AND s->'entity_ids' ? (e->>'entity_id')
            AND s->'features' ? (p_document->'payload'->>'feature')
            AND s->'capability'->>'status'='VERIFIED' AND s->'capability'->>'revision'=s->>'rights_revision'
            AND length(btrim(s->'capability'->>'evidence_reference'))>0 AND (s->'capability'->>'credit_cost')::bigint>0
            AND isfinite((s->'capability'->>'reviewed_at')::timestamptz) AND (s->'capability'->>'reviewed_at')::timestamptz<=now_at
            AND isfinite((s->'capability'->>'valid_until')::timestamptz) AND (s->'capability'->>'valid_until')::timestamptz>now_at
            AND (s->'rights'->>'retention_hours')::integer BETWEEN 1 AND 87840
            AND isfinite((e->>'expires_at')::timestamptz) AND isfinite((e->>'received_at')::timestamptz)) IS NOT TRUE THEN RETURN NULL; END IF;
        candidate:=least((e->>'expires_at')::timestamptz,
            (e->>'received_at')::timestamptz+(s->'rights'->>'retention_hours')::integer*interval '1 hour',
            (s->'capability'->>'valid_until')::timestamptz);
        deadline:=least(deadline,candidate);
    END LOOP;
    RETURN deadline;
EXCEPTION WHEN invalid_text_representation OR datetime_field_overflow OR invalid_datetime_format OR numeric_value_out_of_range THEN RETURN NULL;
END $$;

CREATE FUNCTION research.intelligence_marker_retention(p_document jsonb,p_config jsonb) RETURNS timestamptz
LANGUAGE plpgsql STABLE SET search_path=pg_catalog,pg_temp AS $$
DECLARE e record; s jsonb; deadline timestamptz:='infinity';
BEGIN
    IF (p_document->'scope'=p_config->'scope' AND jsonb_typeof(p_document->'source_revisions')='object'
        AND p_document->'source_revisions'<>'{}'::jsonb) IS NOT TRUE THEN RETURN NULL; END IF;
    FOR e IN SELECT * FROM jsonb_each_text(p_document->'source_revisions') LOOP
        SELECT value INTO s FROM jsonb_array_elements(p_config->'sources') WHERE value->>'source_id'=e.key;
        IF (s->'enabled'='true'::jsonb AND s->'audit_markers'='true'::jsonb AND s->>'rights_revision'=e.value
            AND s->'capability'->>'status'='VERIFIED' AND s->'capability'->>'revision'=e.value
            AND (s->'capability'->>'reviewed_at')::timestamptz<=statement_timestamp()
            AND (s->'capability'->>'valid_until')::timestamptz>statement_timestamp()
            AND (s->'rights'->>'retention_hours')::integer BETWEEN 1 AND 87840) IS NOT TRUE THEN RETURN NULL; END IF;
        deadline:=least(deadline,(p_document->>'rejected_at')::timestamptz+(s->'rights'->>'retention_hours')::integer*interval '1 hour',
            (s->'capability'->>'valid_until')::timestamptz);
    END LOOP;
    RETURN deadline;
EXCEPTION WHEN invalid_text_representation OR datetime_field_overflow OR invalid_datetime_format OR numeric_value_out_of_range THEN RETURN NULL;
END $$;

CREATE FUNCTION research.intelligence_matches(p_kind text,p_target text,p_observation text,p_sources jsonb,p_lineages jsonb) RETURNS boolean
LANGUAGE sql IMMUTABLE SET search_path=pg_catalog,pg_temp AS $$
    SELECT coalesce(CASE p_kind WHEN 'OBSERVATION' THEN p_observation=p_target WHEN 'SOURCE' THEN p_sources ? p_target
        WHEN 'LINEAGE' THEN p_lineages ? p_target ELSE false END,false)
$$;

CREATE OR REPLACE FUNCTION research.guard_intelligence_erasure() RETURNS trigger
LANGUAGE plpgsql SET search_path=pg_catalog,pg_temp AS $$
BEGIN
    PERFORM research.require_read_committed();
    IF (to_jsonb(NEW)-ARRAY['document','retain_until','canonical']) IS DISTINCT FROM (to_jsonb(OLD)-ARRAY['document','retain_until','canonical'])
       OR NEW.retain_until>OLD.retain_until OR OLD.document IS NULL
       OR NOT (NEW.document IS NULL OR (NEW.document=OLD.document AND NEW.retain_until<OLD.retain_until)) THEN
        RAISE EXCEPTION 'intelligence documents are immutable except erasure or shorter retention'; END IF;
    IF NEW.document IS NULL THEN NEW.canonical:=NULL;
    ELSIF NEW.canonical IS DISTINCT FROM OLD.canonical THEN RAISE EXCEPTION 'immutable intelligence canonical content'; END IF;
    RETURN NEW;
END $$;
CREATE FUNCTION research.erase_intelligence_dependents() RETURNS trigger
LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog,pg_temp AS $$
BEGIN
    IF NEW.document IS NULL THEN
        DELETE FROM research.intelligence_assessments WHERE owner_id=NEW.owner_id AND account_id=NEW.account_id AND observation_id=NEW.observation_id;
        DELETE FROM research.intelligence_rejections WHERE owner_id=NEW.owner_id AND account_id=NEW.account_id AND observation_id=NEW.observation_id;
    ELSE
        UPDATE research.intelligence_assessments SET retain_until=least(retain_until,NEW.retain_until)
            WHERE owner_id=NEW.owner_id AND account_id=NEW.account_id AND observation_id=NEW.observation_id;
    END IF;
    RETURN NEW;
END $$;
CREATE TRIGGER intelligence_erase_dependents AFTER UPDATE ON research.intelligence_observations
FOR EACH ROW EXECUTE FUNCTION research.erase_intelligence_dependents();

CREATE FUNCTION research.guard_intelligence_invalidation() RETURNS trigger
LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog,pg_temp AS $$
BEGIN
    PERFORM research.require_read_committed();
    PERFORM pg_advisory_xact_lock(hashtextextended(NEW.owner_id||':'||NEW.account_id,7910));
    IF (research.intelligence_keys(NEW.document,ARRAY['scope','target','target_id','at','reason','erase_content'])
        AND NEW.document->'scope'=jsonb_build_object('owner_id',NEW.owner_id,'account_id',NEW.account_id)
        AND NEW.owner_id ~ '^[A-Za-z0-9_-]{1,96}$' AND NEW.account_id ~ '^[A-Za-z0-9_-]{1,96}$'
        AND NEW.target_id ~ '^[A-Za-z0-9_-]{1,96}$' AND NEW.document->>'target'=NEW.target_kind
        AND NEW.document->>'target_id'=NEW.target_id
        -- Chrono binary timestamps truncate nanoseconds while PostgreSQL's text
        -- parser rounds. Accept only that single-microsecond representation gap.
        AND abs(extract(epoch FROM ((NEW.document->>'at')::timestamptz-NEW.invalidated_at)))<=0.000001
        AND isfinite(NEW.invalidated_at) AND NEW.invalidated_at<=clock_timestamp()
        AND NEW.document->'erase_content'=to_jsonb(NEW.erase_content)
        AND NEW.document->>'reason' IN ('CORRECTION','DELETED','RIGHTS_REVOKED','ENTITLEMENT_CHANGED')
        AND (NEW.document->>'reason' NOT IN ('DELETED','RIGHTS_REVOKED') OR NEW.erase_content)) IS NOT TRUE THEN
        RAISE EXCEPTION 'INVALID_INTELLIGENCE_INVALIDATION'; END IF;
    RETURN NEW;
END $$;
CREATE TRIGGER intelligence_validate_invalidation BEFORE INSERT ON research.intelligence_invalidations
FOR EACH ROW EXECUTE FUNCTION research.guard_intelligence_invalidation();
CREATE FUNCTION research.apply_intelligence_invalidation() RETURNS trigger
LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog,pg_temp AS $$
BEGIN
    DELETE FROM research.intelligence_assessments a WHERE a.owner_id=NEW.owner_id AND a.account_id=NEW.account_id
        AND research.intelligence_matches(NEW.target_kind,NEW.target_id,a.observation_id,a.source_ids,a.lineage_ids)
        AND (NEW.erase_content OR (a.document->>'decision_at')::timestamptz>=NEW.invalidated_at);
    IF NEW.erase_content THEN
        DELETE FROM research.intelligence_rejections r WHERE r.owner_id=NEW.owner_id AND r.account_id=NEW.account_id
            AND research.intelligence_matches(NEW.target_kind,NEW.target_id,r.observation_id,r.source_ids,r.lineage_ids);
        UPDATE research.intelligence_observations o SET document=NULL WHERE o.owner_id=NEW.owner_id AND o.account_id=NEW.account_id
            AND o.document IS NOT NULL AND research.intelligence_matches(NEW.target_kind,NEW.target_id,o.observation_id,o.source_ids,o.lineage_ids);
    END IF;
    RETURN NEW;
END $$;
CREATE TRIGGER intelligence_apply_invalidation AFTER INSERT ON research.intelligence_invalidations
FOR EACH ROW EXECUTE FUNCTION research.apply_intelligence_invalidation();

CREATE FUNCTION research.refresh_intelligence(p_owner text DEFAULT NULL,p_account text DEFAULT NULL) RETURNS void
LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog,pg_temp AS $$
DECLARE scope record; o record; config jsonb; deadline timestamptz; marker jsonb; now_at timestamptz:=clock_timestamp();
BEGIN
    PERFORM research.require_read_committed();
    FOR scope IN SELECT * FROM research.intelligence_scopes WHERE (p_owner IS NULL OR owner_id=p_owner)
        AND (p_account IS NULL OR account_id=p_account) ORDER BY owner_id,account_id LOOP
        PERFORM pg_advisory_xact_lock(hashtextextended(scope.owner_id||':'||scope.account_id,7910));
        -- Re-read after locking, so a concurrent registry replacement cannot be missed.
        SELECT r.document INTO STRICT config FROM research.intelligence_scopes s JOIN research.intelligence_registries r USING(registry_hash)
            WHERE s.owner_id=scope.owner_id AND s.account_id=scope.account_id FOR SHARE OF s;
        FOR o IN SELECT * FROM research.intelligence_observations WHERE owner_id=scope.owner_id AND account_id=scope.account_id AND document IS NOT NULL LOOP
            deadline:=research.intelligence_retention(o.document,config,false);
            IF deadline IS NULL OR least(deadline,o.retain_until)<=now_at THEN
                marker:=jsonb_build_object('scope',jsonb_build_object('owner_id',o.owner_id,'account_id',o.account_id),
                    'target','OBSERVATION','target_id',o.observation_id,'at',now_at,'reason','ENTITLEMENT_CHANGED','erase_content',true);
                INSERT INTO research.intelligence_invalidations(invalidation_id,owner_id,account_id,target_kind,target_id,invalidated_at,erase_content,document)
                    VALUES(weekly.hash(marker::text),o.owner_id,o.account_id,'OBSERVATION',o.observation_id,now_at,true,marker) ON CONFLICT DO NOTHING;
            ELSIF deadline<o.retain_until THEN
                UPDATE research.intelligence_observations SET retain_until=deadline
                    WHERE owner_id=o.owner_id AND account_id=o.account_id AND observation_id=o.observation_id;
            END IF;
        END LOOP;
        DELETE FROM research.intelligence_assessments a USING research.intelligence_observations stored
            WHERE a.owner_id=scope.owner_id AND a.account_id=scope.account_id AND stored.owner_id=a.owner_id AND stored.account_id=a.account_id
                AND stored.observation_id=a.observation_id AND (a.retain_until<=now_at OR stored.document IS NULL
                OR coalesce(research.intelligence_retention(stored.document,config,true),'-infinity'::timestamptz)<=now_at);
        DELETE FROM research.intelligence_rejections r WHERE r.owner_id=scope.owner_id AND r.account_id=scope.account_id
            AND (r.retain_until<=now_at OR coalesce(research.intelligence_marker_retention(r.document,config),'-infinity'::timestamptz)<=now_at);
    END LOOP;
END $$;

CREATE FUNCTION research.activate_intelligence_registry(p_hash text) RETURNS void
LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog,pg_temp AS $$
DECLARE config jsonb; owner text; account text; previous_hash text;
BEGIN
    PERFORM research.require_read_committed();
    SELECT document INTO STRICT config FROM research.intelligence_registries WHERE registry_hash=p_hash;
    owner:=config->'scope'->>'owner_id'; account:=config->'scope'->>'account_id';
    PERFORM pg_advisory_xact_lock(hashtextextended(owner||':'||account,7910));
    IF EXISTS(SELECT 1 FROM research.intelligence_registry_retirements WHERE registry_hash=p_hash) THEN
        RAISE EXCEPTION 'RETIRED_INTELLIGENCE_REGISTRY'; END IF;
    SELECT registry_hash INTO previous_hash FROM research.intelligence_scopes WHERE owner_id=owner AND account_id=account;
    IF previous_hash IS NOT NULL AND previous_hash<>p_hash THEN
        INSERT INTO research.intelligence_registry_retirements(registry_hash,replacement_hash) VALUES(previous_hash,p_hash);
    END IF;
    INSERT INTO research.intelligence_scopes VALUES(owner,account,p_hash)
        ON CONFLICT(owner_id,account_id) DO UPDATE SET registry_hash=excluded.registry_hash;
    PERFORM research.refresh_intelligence(owner,account);
END $$;
CREATE FUNCTION research.guard_intelligence_registry() RETURNS trigger
LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog,pg_temp AS $$
DECLARE s jsonb;
BEGIN
    PERFORM research.require_read_committed();
    IF (research.intelligence_keys(NEW.document,ARRAY['schema_version','scope','enabled','sources','entity_aliases','cross_asset_context','max_input_bytes','max_query_records'])
        AND NEW.document->'schema_version'='1'::jsonb AND jsonb_typeof(NEW.document->'scope')='object'
        AND research.intelligence_keys(NEW.document->'scope',ARRAY['owner_id','account_id'])
        AND NEW.document->'scope'->>'owner_id' ~ '^[A-Za-z0-9_-]{1,96}$'
        AND NEW.document->'scope'->>'account_id' ~ '^[A-Za-z0-9_-]{1,96}$'
        AND jsonb_typeof(NEW.document->'enabled')='array' AND jsonb_typeof(NEW.document->'sources')='array'
        AND jsonb_array_length(NEW.document->'sources')<=128) IS NOT TRUE THEN RAISE EXCEPTION 'INVALID_INTELLIGENCE_REGISTRY'; END IF;
    IF (SELECT count(DISTINCT item->>'source_id') FROM jsonb_array_elements(NEW.document->'sources') item)
        <>jsonb_array_length(NEW.document->'sources') THEN RAISE EXCEPTION 'DUPLICATE_INTELLIGENCE_SOURCE'; END IF;
    FOR s IN SELECT value FROM jsonb_array_elements(NEW.document->'sources') LOOP
        IF (s->>'source_id' ~ '^[A-Za-z0-9_-]{1,96}$' AND jsonb_typeof(s->'enabled')='boolean'
            AND jsonb_typeof(s->'audit_markers')='boolean' AND jsonb_typeof(s->'derived_storage')='boolean'
            AND jsonb_typeof(s->'rights')='object' AND jsonb_typeof(s->'capability')='object'
            AND jsonb_typeof(s->'entity_ids')='array' AND jsonb_typeof(s->'features')='array'
            AND (s->'rights'->>'retention_hours')::integer BETWEEN 1 AND 87840) IS NOT TRUE THEN
            RAISE EXCEPTION 'INVALID_INTELLIGENCE_SOURCE'; END IF;
    END LOOP;
    RETURN NEW;
END $$;
CREATE TRIGGER intelligence_validate_registry BEFORE INSERT ON research.intelligence_registries
FOR EACH ROW EXECUTE FUNCTION research.guard_intelligence_registry();
CREATE FUNCTION research.intelligence_registry_inserted() RETURNS trigger
LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog,pg_temp AS $$
BEGIN PERFORM research.activate_intelligence_registry(NEW.registry_hash); RETURN NEW; END $$;
CREATE TRIGGER intelligence_activate_registry AFTER INSERT ON research.intelligence_registries
FOR EACH ROW EXECUTE FUNCTION research.intelligence_registry_inserted();

CREATE FUNCTION research.guard_intelligence_observation() RETURNS trigger
LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog,pg_temp AS $$
DECLARE config jsonb; e jsonb; sources jsonb; lineages jsonb; deadline timestamptz;
BEGIN
    PERFORM research.require_read_committed();
    PERFORM pg_advisory_xact_lock(hashtextextended(NEW.owner_id||':'||NEW.account_id,7910));
    IF NEW.canonical IS NULL OR octet_length(NEW.canonical)>4194304 THEN RAISE EXCEPTION 'INVALID_INTELLIGENCE_OBSERVATION'; END IF;
    IF (research.intelligence_keys(NEW.document,ARRAY['schema_version','observation_id','scope','entity_id','payload_hash','evidence','supersedes','payload'])
        AND NEW.document->'schema_version'='1'::jsonb AND NEW.document->>'observation_id'=NEW.observation_id
        AND NEW.document->'scope'=jsonb_build_object('owner_id',NEW.owner_id,'account_id',NEW.account_id)
        AND NEW.document->>'entity_id'=NEW.entity_id AND NEW.entity_id ~ '^[A-Za-z0-9_-]{1,96}$'
        AND NEW.document->>'payload_hash' ~ '^[a-f0-9]{64}$' AND research.intelligence_no_tax(NEW.document)
        AND research.intelligence_keys(NEW.document->'payload',ARRAY['feature','input'])
        AND NEW.document->'payload'->>'feature'=NEW.feature AND research.intelligence_input_keys(NEW.feature,NEW.document->'payload'->'input')
        AND jsonb_typeof(NEW.document->'evidence')='array' AND jsonb_array_length(NEW.document->'evidence') BETWEEN 1 AND 64
        AND NEW.document ? 'supersedes' AND (NEW.document->'supersedes'='null'::jsonb OR
            (NEW.document->>'supersedes' ~ '^[A-Za-z0-9_-]{1,96}$' AND NEW.document->>'supersedes'<>NEW.observation_id))
        AND isfinite(NEW.ingested_at) AND NEW.ingested_at<=clock_timestamp() AND isfinite(NEW.retain_until)
        AND NEW.retain_until>clock_timestamp()) IS NOT TRUE THEN RAISE EXCEPTION 'INVALID_INTELLIGENCE_OBSERVATION'; END IF;
    FOR e IN SELECT value FROM jsonb_array_elements(NEW.document->'evidence') LOOP
        IF (research.intelligence_keys(e,ARRAY['source_id','provider','rights_revision','entity_id','currency','unit','venue','adjustment_basis',
            'observed_at','published_at','available_at','received_at','expires_at','source_reference','content_hash','lineage_ids','complete','gaps'])
            AND e->>'source_id' ~ '^[A-Za-z0-9_-]{1,96}$' AND research.intelligence_ids(e->'lineage_ids')
            AND jsonb_array_length(e->'lineage_ids')<=256
            AND e->>'content_hash' ~ '^[a-f0-9]{64}$' AND length(e->>'source_reference') BETWEEN 1 AND 2048
            AND length(e->>'unit') BETWEEN 1 AND 96 AND length(e->>'adjustment_basis') BETWEEN 1 AND 128
            AND jsonb_typeof(e->'complete')='boolean' AND jsonb_typeof(e->'gaps')='array' AND jsonb_array_length(e->'gaps')<=32
            AND isfinite((e->>'observed_at')::timestamptz) AND isfinite((e->>'published_at')::timestamptz)
            AND isfinite((e->>'available_at')::timestamptz) AND isfinite((e->>'received_at')::timestamptz)
            AND (e->>'observed_at')::timestamptz<=(e->>'published_at')::timestamptz
            AND (e->>'published_at')::timestamptz<=(e->>'available_at')::timestamptz
            AND (e->>'available_at')::timestamptz<=(e->>'received_at')::timestamptz
            AND (e->>'received_at')::timestamptz<=NEW.ingested_at
            AND (e->>'received_at')::timestamptz<(e->>'expires_at')::timestamptz) IS NOT TRUE THEN
            RAISE EXCEPTION 'INVALID_INTELLIGENCE_EVIDENCE'; END IF;
    END LOOP;
    IF (NEW.canonical::jsonb=NEW.document AND octet_length(NEW.canonical)<=4194304
        AND NEW.content_hash=weekly.hash(NEW.canonical)
        AND NEW.document->>'payload_hash'=weekly.hash((NEW.canonical::json->'payload')::text)) IS NOT TRUE
        OR NOT research.intelligence_payload_bound(NEW.document->'payload',NEW.document->'evidence') THEN
        RAISE EXCEPTION 'MISBOUND_INTELLIGENCE_CONTENT'; END IF;
    SELECT jsonb_agg(id ORDER BY id COLLATE "C") INTO sources FROM (SELECT DISTINCT evidence->>'source_id' id FROM jsonb_array_elements(NEW.document->'evidence') evidence) ids;
    SELECT jsonb_agg(id ORDER BY id COLLATE "C") INTO lineages FROM (SELECT DISTINCT v#>>'{}' id FROM jsonb_array_elements(NEW.document->'evidence') evidence,
        LATERAL jsonb_array_elements(evidence->'lineage_ids') v) ids;
    IF NEW.source_ids IS DISTINCT FROM sources OR NEW.lineage_ids IS DISTINCT FROM lineages THEN RAISE EXCEPTION 'MISBOUND_INTELLIGENCE_DEPENDENCIES'; END IF;
    SELECT r.document INTO STRICT config FROM research.intelligence_scopes s JOIN research.intelligence_registries r USING(registry_hash)
        WHERE s.owner_id=NEW.owner_id AND s.account_id=NEW.account_id AND s.registry_hash=NEW.registry_hash FOR SHARE OF s;
    deadline:=research.intelligence_retention(NEW.document,config,false);
    IF deadline IS NULL OR deadline<=clock_timestamp() THEN RAISE EXCEPTION 'INTELLIGENCE_RIGHTS_UNAVAILABLE'; END IF;
    NEW.retain_until:=least(NEW.retain_until,deadline);
    -- Erasure is a content tombstone, not merely an observation-ID reservation.
    -- Lock matching history against concurrent erasure. The isolation guard also
    -- covers invalidations and tombstones absent from an older transaction snapshot.
    PERFORM 1 FROM research.intelligence_observations o WHERE o.owner_id=NEW.owner_id AND o.account_id=NEW.account_id
        AND o.lineage_ids ?| ARRAY(SELECT jsonb_array_elements_text(NEW.lineage_ids)) FOR SHARE;
    IF EXISTS(SELECT 1 FROM research.intelligence_observations o WHERE o.owner_id=NEW.owner_id AND o.account_id=NEW.account_id
        AND o.document IS NULL AND o.lineage_ids ?| ARRAY(SELECT jsonb_array_elements_text(NEW.lineage_ids))) THEN
        RAISE EXCEPTION 'ERASED_INTELLIGENCE_LINEAGE_CANNOT_BE_REIMPORTED'; END IF;
    IF EXISTS(SELECT 1 FROM research.intelligence_invalidations i WHERE i.owner_id=NEW.owner_id AND i.account_id=NEW.account_id
        AND research.intelligence_matches(i.target_kind,i.target_id,NEW.observation_id,NEW.source_ids,NEW.lineage_ids)) THEN
        RAISE EXCEPTION 'INVALIDATED_INTELLIGENCE_CANNOT_BE_REIMPORTED'; END IF;
    IF NEW.document->'supersedes'<>'null'::jsonb AND NOT EXISTS(SELECT 1 FROM research.intelligence_observations o
        WHERE o.owner_id=NEW.owner_id AND o.account_id=NEW.account_id AND o.observation_id=NEW.document->>'supersedes'
        AND o.document IS NOT NULL AND o.feature=NEW.feature AND o.entity_id=NEW.entity_id) THEN
        RAISE EXCEPTION 'SUPERSEDED_INTELLIGENCE_UNAVAILABLE'; END IF;
    RETURN NEW;
END $$;
CREATE TRIGGER intelligence_validate_observation BEFORE INSERT ON research.intelligence_observations
FOR EACH ROW EXECUTE FUNCTION research.guard_intelligence_observation();

CREATE FUNCTION research.intelligence_observation_inserted() RETURNS trigger
LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog,pg_temp AS $$
DECLARE marker jsonb;
BEGIN
    IF NEW.document->'supersedes'<>'null'::jsonb THEN
        marker:=jsonb_build_object('scope',NEW.document->'scope','target','OBSERVATION','target_id',NEW.document->>'supersedes',
            'at',NEW.ingested_at,'reason','CORRECTION','erase_content',false);
        INSERT INTO research.intelligence_invalidations(invalidation_id,owner_id,account_id,target_kind,target_id,invalidated_at,erase_content,document)
            VALUES(weekly.hash(marker::text),NEW.owner_id,NEW.account_id,'OBSERVATION',NEW.document->>'supersedes',NEW.ingested_at,false,marker)
            ON CONFLICT DO NOTHING;
    END IF;
    RETURN NEW;
END $$;
CREATE TRIGGER intelligence_correct_observation AFTER INSERT ON research.intelligence_observations
FOR EACH ROW EXECUTE FUNCTION research.intelligence_observation_inserted();

CREATE FUNCTION research.guard_intelligence_assessment() RETURNS trigger
LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog,pg_temp AS $$
DECLARE o research.intelligence_observations; config jsonb; current_hash text; deadline timestamptz;
BEGIN
    PERFORM research.require_read_committed();
    PERFORM pg_advisory_xact_lock(hashtextextended(NEW.owner_id||':'||NEW.account_id,7910));
    SELECT * INTO STRICT o FROM research.intelligence_observations WHERE owner_id=NEW.owner_id AND account_id=NEW.account_id AND observation_id=NEW.observation_id FOR SHARE;
    SELECT r.document,s.registry_hash INTO STRICT config,current_hash FROM research.intelligence_scopes s JOIN research.intelligence_registries r USING(registry_hash)
        WHERE s.owner_id=NEW.owner_id AND s.account_id=NEW.account_id FOR SHARE OF s;
    deadline:=research.intelligence_retention(o.document,config,true);
    IF (research.intelligence_keys(NEW.document,ARRAY['schema_version','feature_version','assessment_id','feature','scope','entity_id','decision_at',
        'computed_at','expires_at','input_hash','registry_hash','observation_id','status','reasons','evidence','result','calibration','entry_authorized'])
        AND NEW.document->'schema_version'='1'::jsonb AND NEW.document->>'feature_version'='trady-intelligence-v1'
        AND NEW.document->>'assessment_id'=NEW.assessment_id AND NEW.document->>'observation_id'=NEW.observation_id
        AND NEW.document->'scope'=jsonb_build_object('owner_id',NEW.owner_id,'account_id',NEW.account_id)
        AND NEW.document->>'feature'=o.feature AND NEW.document->>'entity_id'=o.entity_id
        AND NEW.document->'entry_authorized'='false'::jsonb AND NEW.document->>'calibration'='RESEARCH_ONLY_UNCALIBRATED'
        AND NEW.document->>'status' IN ('AVAILABLE','PARTIAL','UNAVAILABLE','ABSTAINED','INVALIDATED','REJECTED')
        AND research.intelligence_no_tax(NEW.document) AND research.intelligence_keys(NEW.document->'result',ARRAY['feature','assessment'])
        AND jsonb_typeof(NEW.document->'result'->'assessment')='object'
        AND NEW.document->'result'->>'feature'=o.feature AND NEW.document->'evidence'=o.document->'evidence'
        AND NEW.document->>'input_hash'=o.document->>'payload_hash'
        AND NEW.document->>'registry_hash'=current_hash
        AND isfinite((NEW.document->>'decision_at')::timestamptz) AND isfinite((NEW.document->>'computed_at')::timestamptz)
        AND o.ingested_at<=(NEW.document->>'decision_at')::timestamptz
        AND (NEW.document->>'decision_at')::timestamptz<=(NEW.document->>'computed_at')::timestamptz
        AND (NEW.document->>'computed_at')::timestamptz<=clock_timestamp()
        AND NEW.source_ids=o.source_ids AND NEW.lineage_ids=o.lineage_ids
        AND deadline>clock_timestamp() AND NEW.retain_until>clock_timestamp() AND o.retain_until>clock_timestamp()) IS NOT TRUE THEN
        RAISE EXCEPTION 'INVALID_INTELLIGENCE_ASSESSMENT'; END IF;
    IF EXISTS(SELECT 1 FROM research.intelligence_invalidations i WHERE i.owner_id=NEW.owner_id AND i.account_id=NEW.account_id
        AND research.intelligence_matches(i.target_kind,i.target_id,NEW.observation_id,NEW.source_ids,NEW.lineage_ids)
        AND (i.erase_content OR i.invalidated_at<=(NEW.document->>'decision_at')::timestamptz)) THEN RAISE EXCEPTION 'INVALIDATED_INTELLIGENCE_ASSESSMENT'; END IF;
    NEW.retain_until:=least(NEW.retain_until,o.retain_until,deadline);
    RETURN NEW;
END $$;
CREATE TRIGGER intelligence_validate_assessment BEFORE INSERT ON research.intelligence_assessments
FOR EACH ROW EXECUTE FUNCTION research.guard_intelligence_assessment();

CREATE FUNCTION research.guard_intelligence_rejection() RETURNS trigger
LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog,pg_temp AS $$
DECLARE config jsonb; deadline timestamptz; sources jsonb;
BEGIN
    PERFORM research.require_read_committed();
    PERFORM pg_advisory_xact_lock(hashtextextended(NEW.owner_id||':'||NEW.account_id,7910));
    SELECT r.document INTO STRICT config FROM research.intelligence_scopes s JOIN research.intelligence_registries r USING(registry_hash)
        WHERE s.owner_id=NEW.owner_id AND s.account_id=NEW.account_id FOR SHARE OF s;
    deadline:=research.intelligence_marker_retention(NEW.document,config);
    -- Serialize against direct document erasure while permitting markers for inputs
    -- that were rejected before any observation existed.
    PERFORM 1 FROM research.intelligence_observations WHERE owner_id=NEW.owner_id AND account_id=NEW.account_id
        AND observation_id=NEW.observation_id FOR SHARE;
    IF EXISTS(SELECT 1 FROM research.intelligence_observations WHERE owner_id=NEW.owner_id AND account_id=NEW.account_id
        AND observation_id=NEW.observation_id AND document IS NULL) THEN RAISE EXCEPTION 'ERASED_INTELLIGENCE_REJECTION'; END IF;
    PERFORM 1 FROM research.intelligence_observations o WHERE o.owner_id=NEW.owner_id AND o.account_id=NEW.account_id
        AND o.lineage_ids ?| ARRAY(SELECT jsonb_array_elements_text(NEW.lineage_ids)) FOR SHARE;
    IF EXISTS(SELECT 1 FROM research.intelligence_observations o WHERE o.owner_id=NEW.owner_id AND o.account_id=NEW.account_id
        AND o.document IS NULL AND o.lineage_ids ?| ARRAY(SELECT jsonb_array_elements_text(NEW.lineage_ids))) THEN
        RAISE EXCEPTION 'ERASED_INTELLIGENCE_REJECTION'; END IF;
    SELECT jsonb_agg(key ORDER BY key COLLATE "C") INTO sources FROM jsonb_object_keys(NEW.document->'source_revisions') key;
    IF (research.intelligence_keys(NEW.document,ARRAY['scope','observation_id','feature','input_hash','code','rejected_at','retain_until','source_revisions','lineage_ids'])
        AND research.intelligence_feature(NEW.document->>'feature') AND research.intelligence_no_tax(NEW.document)
        AND NEW.document->'scope'=jsonb_build_object('owner_id',NEW.owner_id,'account_id',NEW.account_id)
        AND NEW.document->>'observation_id' IS NOT DISTINCT FROM NEW.observation_id
        AND NEW.document->>'input_hash' ~ '^[a-f0-9]{64}$' AND NEW.document->>'code' ~ '^[A-Za-z0-9_-]{1,96}$'
        AND research.intelligence_ids(NEW.source_ids) AND research.intelligence_ids(NEW.lineage_ids)
        AND jsonb_array_length(NEW.source_ids)<=64 AND jsonb_array_length(NEW.lineage_ids)<=16384
        AND NEW.source_ids=sources AND NEW.document->'lineage_ids'=NEW.lineage_ids
        AND isfinite((NEW.document->>'rejected_at')::timestamptz) AND (NEW.document->>'rejected_at')::timestamptz<=clock_timestamp()
        AND isfinite(NEW.retain_until)
        AND abs(extract(epoch FROM ((NEW.document->>'retain_until')::timestamptz-NEW.retain_until)))<=0.000001
        AND NEW.retain_until>clock_timestamp() AND deadline>clock_timestamp() AND NEW.retain_until<=deadline) IS NOT TRUE THEN
        RAISE EXCEPTION 'INVALID_INTELLIGENCE_REJECTION'; END IF;
    IF EXISTS(SELECT 1 FROM research.intelligence_invalidations i WHERE i.owner_id=NEW.owner_id AND i.account_id=NEW.account_id AND i.erase_content
        AND research.intelligence_matches(i.target_kind,i.target_id,NEW.observation_id,NEW.source_ids,NEW.lineage_ids)) THEN
        RAISE EXCEPTION 'ERASED_INTELLIGENCE_REJECTION'; END IF;
    RETURN NEW;
END $$;
CREATE TRIGGER intelligence_validate_rejection BEFORE INSERT ON research.intelligence_rejections
FOR EACH ROW EXECUTE FUNCTION research.guard_intelligence_rejection();

-- Backfill the current registry for existing scopes, then reconcile old payloads.
INSERT INTO research.intelligence_scopes(owner_id,account_id,registry_hash)
    SELECT DISTINCT ON (document->'scope'->>'owner_id',document->'scope'->>'account_id')
        document->'scope'->>'owner_id',document->'scope'->>'account_id',registry_hash
    FROM research.intelligence_registries ORDER BY document->'scope'->>'owner_id',document->'scope'->>'account_id',recorded_at DESC,registry_hash DESC;
INSERT INTO research.intelligence_registry_retirements(registry_hash,replacement_hash)
    SELECT r.registry_hash,s.registry_hash FROM research.intelligence_registries r JOIN research.intelligence_scopes s
        ON s.owner_id=r.document->'scope'->>'owner_id' AND s.account_id=r.document->'scope'->>'account_id'
        WHERE r.registry_hash<>s.registry_hash;
SELECT research.refresh_intelligence();

CREATE FUNCTION research.intelligence_content_visible(p_owner text,p_account text,p_id text,p_retain timestamptz,p_derived boolean) RETURNS boolean
LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog,pg_temp AS $$
DECLARE config jsonb; o research.intelligence_observations;
BEGIN
    SELECT r.document INTO config FROM research.intelligence_scopes s JOIN research.intelligence_registries r USING(registry_hash)
        WHERE s.owner_id=p_owner AND s.account_id=p_account FOR SHARE OF s;
    IF NOT FOUND THEN RETURN false; END IF;
    SELECT * INTO o FROM research.intelligence_observations WHERE owner_id=p_owner AND account_id=p_account AND observation_id=p_id FOR SHARE;
    RETURN coalesce(p_retain>clock_timestamp() AND o.retain_until>clock_timestamp()
        AND research.intelligence_retention(o.document,config,p_derived)>clock_timestamp(),false);
END
$$;
CREATE FUNCTION research.intelligence_observation_visible(p_owner text,p_account text,p_id text,p_document jsonb,p_retain timestamptz) RETURNS boolean
LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog,pg_temp AS $$
DECLARE config jsonb; current_document jsonb; current_retain timestamptz;
BEGIN
    SELECT r.document INTO config FROM research.intelligence_scopes s JOIN research.intelligence_registries r USING(registry_hash)
        WHERE s.owner_id=p_owner AND s.account_id=p_account FOR SHARE OF s;
    IF NOT FOUND THEN RETURN false; END IF;
    SELECT document,retain_until INTO current_document,current_retain FROM research.intelligence_observations
        WHERE owner_id=p_owner AND account_id=p_account AND observation_id=p_id FOR SHARE;
    -- INSERT RETURNING checks SELECT policy before the inserted row is visible.
    IF NOT FOUND THEN current_document:=p_document; current_retain:=p_retain; END IF;
    RETURN coalesce(p_retain>clock_timestamp() AND current_retain>clock_timestamp()
        AND research.intelligence_retention(current_document,config,false)>clock_timestamp(),false);
END
$$;
CREATE FUNCTION research.intelligence_marker_visible(p_owner text,p_account text,p_id text,p_document jsonb,p_retain timestamptz) RETURNS boolean
LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog,pg_temp AS $$
DECLARE config jsonb;
BEGIN
    SELECT r.document INTO config FROM research.intelligence_scopes s JOIN research.intelligence_registries r USING(registry_hash)
        WHERE s.owner_id=p_owner AND s.account_id=p_account FOR SHARE OF s;
    IF NOT FOUND THEN RETURN false; END IF;
    PERFORM 1 FROM research.intelligence_rejections WHERE rejection_id=p_id FOR SHARE;
    RETURN FOUND AND coalesce(p_retain>clock_timestamp()
        AND research.intelligence_marker_retention(p_document,config)>clock_timestamp(),false);
END
$$;

-- Wall time cannot fire a SQL trigger. RLS denies expired content even while idle;
-- the refresh function physically erases it on the next repository operation/sweep.
ALTER TABLE research.intelligence_observations ENABLE ROW LEVEL SECURITY;
CREATE POLICY intelligence_observations_read ON research.intelligence_observations FOR SELECT TO trady_analyst
    USING (document IS NULL OR research.intelligence_observation_visible(owner_id,account_id,observation_id,document,retain_until));
CREATE POLICY intelligence_observations_insert ON research.intelligence_observations FOR INSERT TO trady_analyst WITH CHECK (true);
CREATE POLICY intelligence_observations_erase ON research.intelligence_observations FOR UPDATE TO trady_analyst USING (true) WITH CHECK (document IS NULL);
ALTER TABLE research.intelligence_assessments ENABLE ROW LEVEL SECURITY;
CREATE POLICY intelligence_assessments_read ON research.intelligence_assessments FOR SELECT TO trady_analyst
    USING (research.intelligence_content_visible(owner_id,account_id,observation_id,retain_until,true));
CREATE POLICY intelligence_assessments_insert ON research.intelligence_assessments FOR INSERT TO trady_analyst WITH CHECK (true);
CREATE POLICY intelligence_assessments_delete ON research.intelligence_assessments FOR DELETE TO trady_analyst USING (true);
ALTER TABLE research.intelligence_rejections ENABLE ROW LEVEL SECURITY;
CREATE POLICY intelligence_rejections_read ON research.intelligence_rejections FOR SELECT TO trady_analyst
    USING (research.intelligence_marker_visible(owner_id,account_id,rejection_id,document,retain_until));
CREATE POLICY intelligence_rejections_insert ON research.intelligence_rejections FOR INSERT TO trady_analyst WITH CHECK (true);
CREATE POLICY intelligence_rejections_delete ON research.intelligence_rejections FOR DELETE TO trady_analyst USING (true);

REVOKE ALL ON FUNCTION research.intelligence_keys(jsonb,text[]),research.intelligence_no_tax(jsonb),research.intelligence_feature(text),
    research.intelligence_ids(jsonb),research.intelligence_input_keys(text,jsonb),
    research.intelligence_payload_bound(jsonb,jsonb,text),research.intelligence_retention(jsonb,jsonb,boolean),research.intelligence_marker_retention(jsonb,jsonb),
    research.intelligence_matches(text,text,text,jsonb,jsonb),research.erase_intelligence_dependents(),research.guard_intelligence_invalidation(),
    research.apply_intelligence_invalidation(),research.refresh_intelligence(text,text),research.activate_intelligence_registry(text),
    research.guard_intelligence_registry(),research.intelligence_registry_inserted(),research.guard_intelligence_observation(),
    research.intelligence_observation_inserted(),research.guard_intelligence_assessment(),research.guard_intelligence_rejection(),
    research.intelligence_content_visible(text,text,text,timestamptz,boolean),research.intelligence_observation_visible(text,text,text,jsonb,timestamptz),
    research.intelligence_marker_visible(text,text,text,jsonb,timestamptz) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION research.refresh_intelligence(text,text),research.activate_intelligence_registry(text),
    research.intelligence_content_visible(text,text,text,timestamptz,boolean),research.intelligence_observation_visible(text,text,text,jsonb,timestamptz),
    research.intelligence_marker_visible(text,text,text,jsonb,timestamptz) TO trady_analyst;
