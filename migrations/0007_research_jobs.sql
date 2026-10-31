-- Durable information work. Neither gateway nor analyst receives executor writes.
ALTER TABLE research.model_sessions DROP CONSTRAINT model_sessions_account_id_check;
ALTER TABLE research.model_sessions ADD CONSTRAINT model_sessions_account_id_check
    CHECK (account_id ~ '^(paper|shadow|live)_[A-Za-z0-9_-]+$');
CREATE TABLE research.job_policies (
    owner_id text NOT NULL,
    account_id text NOT NULL CHECK (account_id ~ '^(paper|shadow|live)_[A-Za-z0-9_-]+$'),
    policy_version integer NOT NULL CHECK (policy_version > 0),
    document jsonb NOT NULL CHECK (jsonb_typeof(document)='object'),
    policy_hash text NOT NULL CHECK (policy_hash ~ '^[a-f0-9]{64}$'),
    updated_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    PRIMARY KEY(owner_id,account_id,policy_version),
    CHECK (document->'scope'->>'owner_id'=owner_id AND document->'scope'->>'account_id'=account_id
        AND (document->'scope'->>'policy_version')::integer=policy_version)
);
CREATE TABLE control.research_work (
    job_id text PRIMARY KEY CHECK (job_id ~ '^job_[a-f0-9]{32}$'),
    owner_id text NOT NULL,
    account_id text NOT NULL,
    policy_version integer NOT NULL,
    idempotency_key text NOT NULL CHECK (length(idempotency_key) BETWEEN 1 AND 160),
    work_hash text NOT NULL CHECK (work_hash ~ '^[a-f0-9]{64}$'),
    policy_hash text NOT NULL CHECK (policy_hash ~ '^[a-f0-9]{64}$'),
    work jsonb NOT NULL CHECK (jsonb_typeof(work)='object'),
    status text NOT NULL DEFAULT 'QUEUED' CHECK (status IN ('QUEUED','RUNNING','COMPLETE','FAILED','CANCELLED')),
    created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    completed_at timestamptz,
    attempts integer NOT NULL DEFAULT 0 CHECK (attempts BETWEEN 0 AND 3),
    lease_worker text,
    lease_generation bigint NOT NULL DEFAULT 0,
    lease_until timestamptz NOT NULL DEFAULT '-infinity',
    result jsonb,
    error_code text,
    UNIQUE(owner_id,account_id,policy_version,idempotency_key),
    FOREIGN KEY(owner_id,account_id,policy_version) REFERENCES research.job_policies
);
CREATE INDEX research_work_pending ON control.research_work(owner_id,account_id,policy_version,created_at)
    WHERE status IN ('QUEUED','RUNNING');
CREATE TABLE research.verified_packets (
    packet_id text PRIMARY KEY CHECK (packet_id ~ '^packet_[a-f0-9]{32}$'),
    owner_id text NOT NULL,
    account_id text NOT NULL,
    policy_version integer NOT NULL,
    instrument_id text NOT NULL,
    packet_hash text NOT NULL CHECK (packet_hash ~ '^[a-f0-9]{64}$'),
    document jsonb NOT NULL,
    created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    expires_at timestamptz NOT NULL,
    FOREIGN KEY(owner_id,account_id,policy_version) REFERENCES research.job_policies
);
CREATE TABLE research.sentiment_assessments (
    assessment_id text PRIMARY KEY CHECK (assessment_id ~ '^sentiment_[a-f0-9]{32}$'),
    job_id text NOT NULL UNIQUE REFERENCES control.research_work,
    packet_id text NOT NULL REFERENCES research.verified_packets,
    owner_id text NOT NULL,
    account_id text NOT NULL,
    policy_version integer NOT NULL,
    model_revision text NOT NULL CHECK (model_revision ~ '^[a-fA-F0-9]{40}$'),
    assessment_hash text NOT NULL CHECK (assessment_hash ~ '^[a-f0-9]{64}$'),
    document jsonb NOT NULL,
    readiness_reference jsonb,
    readiness_blocker text,
    created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    expires_at timestamptz NOT NULL
);
CREATE TABLE research.dependency_invalidations (
    packet_id text NOT NULL REFERENCES research.verified_packets,
    reason text NOT NULL,
    recorded_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    PRIMARY KEY(packet_id,reason)
);
CREATE TABLE control.gateway_receipts (
    bot_id bigint NOT NULL CHECK (bot_id > 0),
    update_id bigint NOT NULL CHECK (update_id >= 0),
    owner_user_id bigint NOT NULL CHECK (owner_user_id > 0),
    chat_id bigint NOT NULL CHECK (chat_id=owner_user_id),
    owner_id text NOT NULL,
    account_id text NOT NULL,
    policy_version integer NOT NULL,
    request_hash text NOT NULL,
    reply text NOT NULL CHECK (octet_length(reply)<=12000),
    job_id text REFERENCES control.research_work,
    created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    PRIMARY KEY(bot_id,update_id)
);
CREATE TABLE control.gateway_outbox (
    delivery_id text PRIMARY KEY,
    bot_id bigint NOT NULL,
    owner_user_id bigint NOT NULL,
    chat_id bigint NOT NULL CHECK(chat_id=owner_user_id),
    owner_id text NOT NULL,
    account_id text NOT NULL,
    policy_version integer NOT NULL,
    job_id text REFERENCES control.research_work,
    text text NOT NULL CHECK (octet_length(text)<=12000),
    created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    delivered_at timestamptz,
    UNIQUE(bot_id,chat_id,job_id)
);
CREATE TABLE control.gateway_sessions (
    bot_id bigint NOT NULL,
    owner_user_id bigint NOT NULL,
    owner_id text NOT NULL,
    account_id text NOT NULL,
    policy_version integer NOT NULL,
    session_id text NOT NULL CHECK (session_id ~ '^session_[a-f0-9]{32}$'),
    PRIMARY KEY(bot_id,owner_user_id,owner_id,account_id,policy_version)
);
CREATE TABLE research.gateway_reports (
    owner_id text NOT NULL,
    account_id text NOT NULL,
    policy_version integer NOT NULL,
    name text NOT NULL,
    subject text NOT NULL DEFAULT '',
    document jsonb NOT NULL,
    observed_at timestamptz NOT NULL,
    expires_at timestamptz NOT NULL,
    PRIMARY KEY(owner_id,account_id,policy_version,name,subject)
);
CREATE VIEW control.research_source_health WITH (security_barrier=true) AS
    SELECT source_id,updated_at,document->'last_success_at' AS last_success_at,
        document->'watermark' AS watermark,document->'retry_at' AS retry_at,
        document->'requests_today' AS requests_today,document->'gaps' AS gaps,
        document->'registry_hash' AS registry_hash FROM research.source_states;
-- Published by executor after reconciliation; gateway can only inspect it.
CREATE TABLE control.owner_control_context (
    account_id text PRIMARY KEY,
    owner_id text NOT NULL,
    policy_version integer NOT NULL,
    document jsonb NOT NULL,
    context_hash text NOT NULL CHECK (context_hash ~ '^[a-f0-9]{64}$'),
    observed_at timestamptz NOT NULL
);
CREATE TABLE control.owner_controls (
    command_id text PRIMARY KEY CHECK (command_id ~ '^control_[a-f0-9]{32}$'),
    bot_id bigint NOT NULL,
    update_id bigint NOT NULL,
    owner_user_id bigint NOT NULL,
    chat_id bigint NOT NULL CHECK (chat_id=owner_user_id),
    owner_id text NOT NULL,
    account_id text NOT NULL,
    policy_version integer NOT NULL,
    kind text NOT NULL CHECK (kind IN ('PAUSE_ENTRIES','RESUME_ENTRIES','CLOSE_POSITION')),
    payload jsonb NOT NULL,
    context_hash text NOT NULL,
    token_hash text UNIQUE CHECK (token_hash IS NULL OR token_hash ~ '^[a-f0-9]{64}$'),
    expires_at timestamptz NOT NULL,
    confirmed_at timestamptz,
    status text NOT NULL CHECK (status IN ('PENDING','REQUESTED','ACCEPTED','SUBMITTED','PARTIAL','COMPLETE','REJECTED','CANCELLED')),
    result jsonb,
    producer text NOT NULL DEFAULT session_user CHECK (producer='trady_gateway'),
    created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    UNIQUE(bot_id,update_id)
);
ALTER TABLE control.gateway_outbox ADD COLUMN command_id text REFERENCES control.owner_controls;
ALTER TABLE control.gateway_outbox ADD CHECK (num_nonnulls(job_id,command_id)=1);
CREATE FUNCTION control.notify_owner_control_result() RETURNS trigger
LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog,pg_temp AS $$
BEGIN
    IF NEW.status IN ('ACCEPTED','SUBMITTED','PARTIAL','COMPLETE','REJECTED','CANCELLED')
        AND (NEW.status IS DISTINCT FROM OLD.status OR NEW.result IS DISTINCT FROM OLD.result) THEN
        INSERT INTO control.gateway_outbox(delivery_id,bot_id,owner_user_id,chat_id,owner_id,account_id,policy_version,command_id,text)
            VALUES('control_delivery_'||md5(NEW.command_id||':'||NEW.status||':'||coalesce(NEW.result::text,'')),
                NEW.bot_id,NEW.owner_user_id,NEW.chat_id,NEW.owner_id,NEW.account_id,NEW.policy_version,NEW.command_id,
                'Owner control '||NEW.command_id||' '||NEW.status||E'\n'||left(coalesce(NEW.result::text,'Executor has not supplied further result details.'),2800))
            ON CONFLICT DO NOTHING;
    END IF;
    RETURN NEW;
END;
$$;
CREATE TRIGGER owner_control_result_outbox AFTER UPDATE ON control.owner_controls
    FOR EACH ROW EXECUTE FUNCTION control.notify_owner_control_result();
REVOKE ALL ON FUNCTION control.notify_owner_control_result() FROM PUBLIC;
REVOKE ALL ON research.job_policies,research.verified_packets,research.sentiment_assessments,research.dependency_invalidations,
    control.research_work,control.gateway_receipts,control.gateway_outbox,control.gateway_sessions,
    control.owner_control_context,control.owner_controls FROM PUBLIC;
GRANT SELECT,INSERT,UPDATE ON research.job_policies TO trady_analyst;
GRANT SELECT ON research.job_policies TO trady_gateway,trady_executor;
GRANT SELECT ON control.research_work TO trady_gateway,trady_analyst;
GRANT UPDATE(status,completed_at,attempts,lease_worker,lease_generation,lease_until,result,error_code) ON control.research_work TO trady_analyst;
GRANT SELECT,INSERT ON research.verified_packets,research.sentiment_assessments,research.dependency_invalidations TO trady_analyst;
REVOKE ALL ON research.gateway_reports,control.research_source_health FROM PUBLIC;
GRANT SELECT,INSERT,UPDATE ON research.gateway_reports TO trady_analyst,trady_executor;
GRANT SELECT ON research.gateway_reports,control.research_source_health TO trady_gateway;
GRANT SELECT ON control.research_source_health TO trady_analyst;
GRANT SELECT ON research.verified_packets,research.sentiment_assessments,research.dependency_invalidations TO trady_executor,trady_gateway;
GRANT SELECT,INSERT ON control.gateway_receipts TO trady_gateway;
GRANT SELECT ON control.gateway_receipts TO trady_analyst;
GRANT SELECT,INSERT ON control.gateway_outbox TO trady_analyst;
GRANT SELECT,UPDATE(delivered_at) ON control.gateway_outbox TO trady_gateway;
GRANT SELECT,INSERT,UPDATE ON control.gateway_sessions TO trady_gateway,trady_analyst;
GRANT SELECT ON control.owner_control_context TO trady_gateway,trady_analyst;
GRANT SELECT,INSERT,UPDATE ON control.owner_control_context TO trady_executor;
GRANT SELECT,INSERT ON control.owner_controls TO trady_gateway;
GRANT UPDATE(status,confirmed_at) ON control.owner_controls TO trady_gateway;
GRANT SELECT ON control.owner_controls TO trady_executor;
GRANT UPDATE(status,result) ON control.owner_controls TO trady_executor;
GRANT SELECT ON control.research_work TO trady_executor;

-- The gateway cannot insert arbitrary work: scope, revision, cost and replay
-- limits are enforced inside this bounded information-only function as well.
CREATE FUNCTION control.enqueue_information_work(p_owner text,p_account text,p_policy integer,
    p_key text,p_work jsonb,p_hash text,p_policy_hash text,p_job text) RETURNS text
LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog,pg_temp AS $$
DECLARE pol jsonb; approved_hash text; existing text; old_hash text; k text; n bigint; pending bigint;
BEGIN
    IF session_user NOT IN ('trady_gateway','trady_analyst') OR length(p_key) NOT BETWEEN 1 AND 160
        OR p_job !~ '^job_[a-f0-9]{32}$' OR p_hash !~ '^[a-f0-9]{64}$'
        OR p_policy_hash !~ '^[a-f0-9]{64}$' THEN RAISE EXCEPTION 'invalid information work'; END IF;
    PERFORM pg_advisory_xact_lock(hashtextextended(p_owner||':'||p_account||':'||p_policy,7307));
    SELECT document,policy_hash INTO STRICT pol,approved_hash FROM research.job_policies
        WHERE owner_id=p_owner AND account_id=p_account AND policy_version=p_policy FOR SHARE;
    IF pol->>'enabled'<>'true' OR approved_hash<>p_policy_hash THEN RAISE EXCEPTION 'research scope disabled or changed'; END IF;
    SELECT job_id,work_hash INTO existing,old_hash FROM control.research_work
        WHERE owner_id=p_owner AND account_id=p_account AND policy_version=p_policy AND idempotency_key=p_key;
    IF existing IS NOT NULL THEN
        IF old_hash<>p_hash THEN RAISE EXCEPTION 'idempotency payload mismatch'; END IF;
        RETURN existing;
    END IF;
    k:=p_work->>'kind';
    IF k NOT IN ('RESEARCH','SOURCE_LOOKUP','COLLECT_SOURCE','SENTIMENT','DIALOGUE','SESSION','DAILY','HOLDING_WATCHDOG')
        OR octet_length(p_work::text)>6000 THEN RAISE EXCEPTION 'unsupported information work'; END IF;
    IF k IN ('RESEARCH','HOLDING_WATCHDOG') AND p_work->>'instrument_id' IS NOT NULL
        AND NOT (pol->'instrument_ids' ? (p_work->>'instrument_id')) THEN RAISE EXCEPTION 'instrument outside scope'; END IF;
    IF k='SOURCE_LOOKUP' AND (NOT (pol->'source_ids' ? (p_work->>'source_id'))
        OR p_work->>'external_id' !~ '^[A-Za-z0-9_-]{1,128}$') THEN RAISE EXCEPTION 'source outside scope'; END IF;
    IF k='COLLECT_SOURCE' AND NOT (pol->'source_ids' ? (p_work->>'source_id')) THEN RAISE EXCEPTION 'collector source outside scope'; END IF;
    IF k='SENTIMENT' AND (pol->>'sentiment_revision' IS NULL
        OR p_work->>'model_revision' IS DISTINCT FROM pol->>'sentiment_revision'
        OR NOT EXISTS (SELECT 1 FROM research.verified_packets WHERE packet_id=p_work->>'packet_id'
            AND owner_id=p_owner AND account_id=p_account AND policy_version=p_policy AND expires_at>clock_timestamp()))
        THEN RAISE EXCEPTION 'sentiment packet or revision unavailable'; END IF;
    IF k='DIALOGUE' AND (length(btrim(p_work->>'text')) NOT BETWEEN 1 AND 4096
        OR (p_work->>'session_id' IS NOT NULL AND p_work->>'session_id' !~ '^session_[a-f0-9]{32}$'))
        THEN RAISE EXCEPTION 'invalid dialogue work'; END IF;
    IF k<>'DIALOGUE' THEN
        SELECT job_id INTO existing FROM control.research_work WHERE owner_id=p_owner AND account_id=p_account
            AND policy_version=p_policy AND work_hash=p_hash AND policy_hash=p_policy_hash
            AND status IN ('QUEUED','RUNNING') ORDER BY created_at LIMIT 1;
        IF existing IS NOT NULL THEN RETURN existing; END IF;
    END IF;
    SELECT count(*) FILTER (WHERE created_at>=date_trunc('day',clock_timestamp() AT TIME ZONE 'UTC') AT TIME ZONE 'UTC'),
        count(*) FILTER (WHERE status IN ('QUEUED','RUNNING')) INTO n,pending FROM control.research_work
        WHERE owner_id=p_owner AND account_id=p_account AND policy_version=p_policy;
    IF n>=(pol->>'daily_job_limit')::integer OR pending>=(pol->>'pending_job_limit')::integer
        THEN RAISE EXCEPTION 'research job budget exhausted'; END IF;
    INSERT INTO control.research_work(job_id,owner_id,account_id,policy_version,idempotency_key,work_hash,policy_hash,work)
        VALUES(p_job,p_owner,p_account,p_policy,p_key,p_hash,p_policy_hash,p_work);
    RETURN p_job;
END;
$$;
REVOKE ALL ON FUNCTION control.enqueue_information_work(text,text,integer,text,jsonb,text,text,text) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION control.enqueue_information_work(text,text,integer,text,jsonb,text,text,text) TO trady_gateway,trady_analyst;

-- Immutable reference predicate usable inside the executor's plan transaction.
-- No source text or transcript read grant is conferred on the executor.
CREATE FUNCTION research.sentiment_reference_current(p_reference jsonb) RETURNS boolean
LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog,pg_temp AS $$
DECLARE packet jsonb; dependency jsonb;
BEGIN
    SELECT p.document INTO packet FROM research.sentiment_assessments a
        JOIN research.verified_packets p USING(packet_id)
        JOIN research.job_policies pol ON pol.owner_id=a.owner_id AND pol.account_id=a.account_id AND pol.policy_version=a.policy_version
        WHERE a.assessment_id=p_reference->>'assessment_id' AND a.readiness_reference=p_reference FOR SHARE OF a,p,pol;
    IF packet IS NULL THEN RETURN false; END IF;
    FOR dependency IN SELECT value FROM jsonb_array_elements(packet->'sources') LOOP
        PERFORM i.item_id FROM research.source_items i JOIN research.source_states s USING(source_id)
            WHERE i.item_id=dependency->>'item_id' FOR SHARE OF i,s;
    END LOOP;
    RETURN EXISTS (
        SELECT 1 FROM research.sentiment_assessments a
        JOIN research.verified_packets p USING(packet_id)
        JOIN research.job_policies pol ON pol.owner_id=a.owner_id AND pol.account_id=a.account_id AND pol.policy_version=a.policy_version
        WHERE a.assessment_id=p_reference->>'assessment_id' AND a.readiness_reference=p_reference
        AND a.expires_at>clock_timestamp() AND p.expires_at>clock_timestamp()
        AND (p_reference->>'expires_at')::timestamptz>clock_timestamp()
        AND (p_reference->>'scored_at')::timestamptz<=clock_timestamp()
        AND (p_reference->>'scored_at')::timestamptz>clock_timestamp()-interval '15 minutes'
        AND pol.document->>'enabled'='true'
        AND pol.document->>'registry_hash'=p.document->>'registry_hash'
        AND pol.document->>'sentiment_revision'=a.model_revision
        AND NOT EXISTS (SELECT 1 FROM research.dependency_invalidations inv WHERE inv.packet_id=p.packet_id)
        AND NOT EXISTS (
            SELECT 1 FROM jsonb_array_elements(p.document->'sources') dep
            LEFT JOIN research.source_items i ON i.item_id=dep->>'item_id'
            LEFT JOIN research.source_states s ON s.source_id=i.source_id
            WHERE i.item_id IS NULL OR s.source_id IS NULL OR i.content_hash<>dep->>'content_hash'
            OR i.lineage_id<>dep->>'lineage_id' OR i.source_id<>dep->>'source_id'
            OR i.expires_at<=clock_timestamp() OR i.expires_at<p.expires_at
            OR i.document->>'content_status' NOT IN ('AVAILABLE','SNIPPET')
            OR i.document->'rights'->>'store_content' IS DISTINCT FROM 'true'
            OR i.document->'rights'->>'llm_processing' IS DISTINCT FROM 'true'
            OR i.document->'rights' IS DISTINCT FROM dep->'item_rights'
            OR s.document->>'registry_hash' IS DISTINCT FROM dep->>'source_registry_hash'
            OR s.document->'rights' IS DISTINCT FROM dep->'source_rights'
            OR NOT (pol.document->'source_ids' ? i.source_id)
            OR NOT (pol.document->'instrument_ids' ? (p.document->>'instrument_id'))
            OR COALESCE((i.document->'social'->>'missing_context')::boolean,false)
            OR EXISTS (SELECT 1 FROM research.source_invalidations inv WHERE inv.item_id=i.item_id AND inv.recorded_at>=p.created_at)
        )
    );
END;
$$;
REVOKE ALL ON FUNCTION research.sentiment_reference_current(jsonb) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION research.sentiment_reference_current(jsonb) TO trady_executor,trady_analyst;

-- An invalidation writer participates in the same locks as plan acceptance.
CREATE FUNCTION research.lock_dependency_invalidation() RETURNS trigger
LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog,pg_temp AS $$
BEGIN
    IF TG_TABLE_NAME='source_invalidations' THEN
        PERFORM item_id FROM research.source_items WHERE item_id=NEW.item_id FOR UPDATE;
    ELSE
        PERFORM packet_id FROM research.verified_packets WHERE packet_id=NEW.packet_id FOR UPDATE;
    END IF;
    RETURN NEW;
END;
$$;
REVOKE ALL ON FUNCTION research.lock_dependency_invalidation() FROM PUBLIC;
CREATE TRIGGER source_invalidation_lock BEFORE INSERT ON research.source_invalidations
    FOR EACH ROW EXECUTE FUNCTION research.lock_dependency_invalidation();
CREATE TRIGGER packet_invalidation_lock BEFORE INSERT ON research.dependency_invalidations
    FOR EACH ROW EXECUTE FUNCTION research.lock_dependency_invalidation();
