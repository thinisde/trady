-- Executed transactionally as trady_owner by the dedicated migrator.
REVOKE ALL ON DATABASE trady FROM PUBLIC;
GRANT CONNECT ON DATABASE trady TO trady_migrator, trady_analyst, trady_executor, trady_gateway;
REVOKE ALL ON SCHEMA public FROM PUBLIC;
CREATE SCHEMA policy AUTHORIZATION trady_owner;
CREATE SCHEMA research AUTHORIZATION trady_owner;
CREATE SCHEMA execution AUTHORIZATION trady_owner;
CREATE SCHEMA control AUTHORIZATION trady_owner;
CREATE SCHEMA audit AUTHORIZATION trady_owner;
ALTER DEFAULT PRIVILEGES FOR ROLE trady_owner REVOKE EXECUTE ON FUNCTIONS FROM PUBLIC;

CREATE TABLE policy.versions (
    policy_version integer PRIMARY KEY CHECK (policy_version > 0),
    body jsonb NOT NULL,
    approved_at timestamptz NOT NULL DEFAULT clock_timestamp()
);
CREATE TABLE research.runs (
    run_id text PRIMARY KEY,
    document jsonb NOT NULL,
    created_at timestamptz NOT NULL DEFAULT clock_timestamp()
);
CREATE TABLE execution.accounts (
    account_id text PRIMARY KEY CHECK (account_id ~ '^paper_[A-Za-z0-9_-]+$'),
    mode text NOT NULL DEFAULT 'paper' CHECK (mode = 'paper'),
    currency text NOT NULL CHECK (currency ~ '^[A-Z]{3}$'),
    reconciliation jsonb,
    latest_observed_at timestamptz,
    recovered_generation bigint,
    recovery_required boolean NOT NULL DEFAULT true
);
CREATE TABLE execution.account_leases (
    account_id text PRIMARY KEY REFERENCES execution.accounts,
    worker_id text NOT NULL,
    generation bigint NOT NULL CHECK (generation > 0),
    expires_at timestamptz NOT NULL
);
CREATE TABLE execution.order_intents (
    intent_id text PRIMARY KEY,
    account_id text NOT NULL REFERENCES execution.accounts,
    session_id text NOT NULL,
    state text NOT NULL CHECK (state IN ('PREPARED','UNKNOWN','ACKNOWLEDGED','RESOLVED')),
    document jsonb NOT NULL,
    UNIQUE (account_id, session_id)
);
CREATE TABLE execution.account_snapshots (
    account_id text NOT NULL REFERENCES execution.accounts,
    snapshot_id text NOT NULL,
    document jsonb NOT NULL,
    PRIMARY KEY (account_id, snapshot_id)
);
CREATE TABLE control.research_jobs (
    job_id text PRIMARY KEY,
    payload jsonb NOT NULL,
    producer text NOT NULL DEFAULT session_user CHECK (producer = 'trady_analyst'),
    created_at timestamptz NOT NULL DEFAULT clock_timestamp()
);
CREATE TABLE control.owner_commands (
    command_id text PRIMARY KEY,
    command_kind text NOT NULL CHECK (command_kind IN ('STATUS','PAUSE','CLOSE_REQUEST')),
    owner_user_id text NOT NULL,
    account_id text NOT NULL,
    payload jsonb NOT NULL,
    producer text NOT NULL DEFAULT session_user CHECK (producer = 'trady_gateway'),
    created_at timestamptz NOT NULL DEFAULT clock_timestamp()
);
CREATE TABLE control.notification_outbox (
    event_key text PRIMARY KEY,
    payload jsonb NOT NULL,
    producer text NOT NULL,
    claim_worker text,
    claim_generation bigint NOT NULL DEFAULT 0,
    claim_until timestamptz,
    delivered_at timestamptz,
    created_at timestamptz NOT NULL DEFAULT clock_timestamp()
);
CREATE TABLE audit.events (
    event_id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    actor text NOT NULL,
    event_type text NOT NULL,
    account_id text,
    payload jsonb NOT NULL,
    created_at timestamptz NOT NULL DEFAULT clock_timestamp()
);

CREATE VIEW execution.account_summary AS
    SELECT account_id, mode, currency, recovery_required, latest_observed_at
    FROM execution.accounts;

CREATE FUNCTION execution.register_paper_account(p_account text, p_currency text)
RETURNS void LANGUAGE plpgsql SECURITY DEFINER SET search_path = pg_catalog, pg_temp AS $$
BEGIN
    INSERT INTO execution.accounts(account_id,currency) VALUES(p_account,p_currency)
    ON CONFLICT(account_id) DO NOTHING;
    IF NOT EXISTS (SELECT 1 FROM execution.accounts WHERE account_id=p_account AND currency=p_currency AND mode='paper') THEN
        RAISE EXCEPTION 'account identity mismatch';
    END IF;
END;
$$;

CREATE FUNCTION execution.acquire_lease(p_account text,p_worker text,p_ttl integer)
RETURNS bigint LANGUAGE plpgsql SECURITY DEFINER SET search_path = pg_catalog, pg_temp AS $$
DECLARE next_generation bigint;
BEGIN
    IF p_ttl IS NULL OR p_ttl < 1 OR p_ttl > 300 OR p_worker IS NULL OR p_worker !~ '^[A-Za-z0-9_-]{1,96}$' THEN RAISE EXCEPTION 'invalid lease arguments'; END IF;
    INSERT INTO execution.account_leases(account_id,worker_id,generation,expires_at)
    VALUES(p_account,p_worker,1,clock_timestamp()+make_interval(secs=>p_ttl))
    ON CONFLICT(account_id) DO UPDATE SET worker_id=p_worker,
        generation=execution.account_leases.generation+1,
        expires_at=clock_timestamp()+make_interval(secs=>p_ttl)
    WHERE execution.account_leases.expires_at<=clock_timestamp()
    RETURNING generation INTO next_generation;
    IF next_generation IS NULL THEN RAISE EXCEPTION 'executor lease is busy'; END IF;
    UPDATE execution.accounts SET recovery_required=true,recovered_generation=NULL WHERE account_id=p_account;
    INSERT INTO audit.events(actor,event_type,account_id,payload)
        VALUES(session_user,'LEASE_ACQUIRED',p_account,jsonb_build_object('worker',p_worker,'generation',next_generation));
    RETURN next_generation;
END;
$$;

-- Every financial mutation locks this row and checks expiry against database time.
CREATE FUNCTION execution.assert_fence(p_account text,p_worker text,p_generation bigint)
RETURNS void LANGUAGE plpgsql SECURITY DEFINER SET search_path = pg_catalog, pg_temp AS $$
DECLARE lease execution.account_leases%ROWTYPE;
BEGIN
    SELECT * INTO lease FROM execution.account_leases WHERE account_id=p_account FOR UPDATE;
    IF NOT FOUND OR lease.worker_id IS DISTINCT FROM p_worker OR lease.generation IS DISTINCT FROM p_generation OR lease.expires_at<=clock_timestamp() THEN
        RAISE EXCEPTION 'executor lease lost or fenced';
    END IF;
END;
$$;

CREATE FUNCTION execution.renew_lease(p_account text,p_worker text,p_generation bigint,p_ttl integer)
RETURNS void LANGUAGE plpgsql SECURITY DEFINER SET search_path = pg_catalog, pg_temp AS $$
BEGIN
    IF p_ttl IS NULL OR p_ttl < 1 OR p_ttl > 300 THEN RAISE EXCEPTION 'invalid lease TTL'; END IF;
    PERFORM execution.assert_fence(p_account,p_worker,p_generation);
    UPDATE execution.account_leases SET expires_at=clock_timestamp()+make_interval(secs=>p_ttl) WHERE account_id=p_account;
END;
$$;

CREATE FUNCTION execution.save_reconciliation(p_account text,p_worker text,p_generation bigint,p_document jsonb)
RETURNS void LANGUAGE plpgsql SECURITY DEFINER SET search_path = pg_catalog, pg_temp AS $$
DECLARE observed timestamptz; previous timestamptz; snap_id text; resolution jsonb; old_snapshot jsonb; account_currency text;
BEGIN
    PERFORM execution.assert_fence(p_account,p_worker,p_generation);
    IF (p_document#>>'{snapshot,account_id}') IS DISTINCT FROM p_account OR (p_document#>>'{snapshot,mode}') IS DISTINCT FROM 'paper'
        OR p_document->>'state' NOT IN ('FLAT','ENTRY_PENDING','OPEN_UNPROTECTED','RECOVERY_REQUIRED') THEN RAISE EXCEPTION 'invalid reconciliation identity or state'; END IF;
    observed=(p_document#>>'{snapshot,observed_at}')::timestamptz;
    IF observed IS NULL OR observed>clock_timestamp() OR observed<clock_timestamp()-interval '60 seconds' THEN RAISE EXCEPTION 'stale or future reconciliation'; END IF;
    SELECT latest_observed_at,currency INTO previous,account_currency FROM execution.accounts WHERE account_id=p_account FOR UPDATE;
    IF (p_document#>>'{snapshot,spendable_cash,currency}') IS DISTINCT FROM account_currency THEN RAISE EXCEPTION 'account currency mismatch'; END IF;
    IF previous IS NOT NULL AND observed<previous THEN RAISE EXCEPTION 'snapshot time regression'; END IF;
    snap_id=p_document#>>'{snapshot,snapshot_id}';
    IF snap_id IS NULL OR length(snap_id)=0 THEN RAISE EXCEPTION 'snapshot ID required'; END IF;
    SELECT document INTO old_snapshot FROM execution.account_snapshots WHERE account_id=p_account AND snapshot_id=snap_id;
    IF FOUND AND old_snapshot IS DISTINCT FROM p_document->'snapshot' THEN RAISE EXCEPTION 'snapshot ID reused with different content'; END IF;
    INSERT INTO execution.account_snapshots(account_id,snapshot_id,document) VALUES(p_account,snap_id,p_document->'snapshot') ON CONFLICT DO NOTHING;
    FOR resolution IN SELECT value FROM jsonb_array_elements(p_document->'resolutions') LOOP
        IF resolution->>'state' NOT IN ('UNKNOWN','ACKNOWLEDGED','RESOLVED') THEN RAISE EXCEPTION 'invalid intent resolution'; END IF;
        UPDATE execution.order_intents SET state=resolution->>'state',
            document=jsonb_set(jsonb_set(document,'{state}',resolution->'state'),'{broker_order_id}',resolution->'broker_order_id')
            WHERE account_id=p_account AND intent_id=resolution->>'intent_id' AND state<>'RESOLVED';
        IF NOT FOUND AND NOT EXISTS (SELECT 1 FROM execution.order_intents WHERE account_id=p_account AND intent_id=resolution->>'intent_id' AND state='RESOLVED') THEN RAISE EXCEPTION 'unknown intent resolution'; END IF;
    END LOOP;
    UPDATE execution.accounts SET reconciliation=p_document, latest_observed_at=observed,
        recovered_generation=p_generation,recovery_required=(p_document->>'state'='RECOVERY_REQUIRED') WHERE account_id=p_account;
    INSERT INTO control.notification_outbox(event_key,payload,producer)
        VALUES('snapshot:'||p_account||':'||snap_id,jsonb_build_object('event','ACCOUNT_RECONCILED','account_id',p_account,'state',p_document->>'state'),session_user)
        ON CONFLICT DO NOTHING;
    INSERT INTO audit.events(actor,event_type,account_id,payload)
        VALUES(session_user,'ACCOUNT_RECONCILED',p_account,jsonb_build_object('snapshot_id',snap_id,'generation',p_generation,'state',p_document->>'state'));
END;
$$;

CREATE FUNCTION execution.reserve_paper_intent(p_account text,p_worker text,p_generation bigint,p_intent jsonb)
RETURNS boolean LANGUAGE plpgsql SECURITY DEFINER SET search_path = pg_catalog, pg_temp AS $$
DECLARE account execution.accounts%ROWTYPE; old_intent jsonb; intent_key text;
BEGIN
    PERFORM execution.assert_fence(p_account,p_worker,p_generation);
    SELECT * INTO account FROM execution.accounts WHERE account_id=p_account FOR UPDATE;
    IF account.mode<>'paper' OR p_account !~ '^paper_' OR p_intent->>'account_id' IS DISTINCT FROM p_account OR p_intent->>'side' IS DISTINCT FROM 'BUY'
        OR p_intent->>'state' IS DISTINCT FROM 'PREPARED' OR p_intent->'broker_order_id' IS DISTINCT FROM 'null'::jsonb THEN RAISE EXCEPTION 'only new paper fixture entries are supported'; END IF;
    IF jsonb_typeof(p_intent) IS DISTINCT FROM 'object'
        OR NOT (p_intent ?& ARRAY['intent_id','account_id','instrument_id','side','quantity','limit_price','session_id','created_at','state','broker_order_id'])
        OR (p_intent-ARRAY['intent_id','account_id','instrument_id','side','quantity','limit_price','session_id','created_at','state','broker_order_id'])<>'{}'::jsonb
        OR jsonb_typeof(p_intent->'quantity') IS DISTINCT FROM 'string'
        OR jsonb_typeof(p_intent->'limit_price') IS DISTINCT FROM 'string'
        OR coalesce(p_intent->>'instrument_id','') !~ '^[A-Za-z0-9_-]{1,96}$'
        OR coalesce(p_intent->>'created_at','') !~ '^[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}(\.[0-9]+)?Z$'
        THEN RAISE EXCEPTION 'invalid paper intent fields'; END IF;
    PERFORM (p_intent->>'created_at')::timestamptz;
    IF (p_intent->>'quantity') !~ '^(0|[1-9][0-9]*)(\.[0-9]+)?$'
        OR (p_intent->>'limit_price') !~ '^(0|[1-9][0-9]*)(\.[0-9]+)?$'
        OR (p_intent->>'quantity')::numeric>79228162514264337593543950335
        OR (p_intent->>'limit_price')::numeric>79228162514264337593543950335
        OR scale((p_intent->>'quantity')::numeric)>28 OR scale((p_intent->>'limit_price')::numeric)>28
        THEN RAISE EXCEPTION 'invalid paper financial decimals'; END IF;
    intent_key=p_intent->>'intent_id';
    IF intent_key IS NULL OR intent_key !~ '^[A-Za-z0-9_-]{1,96}$' OR (p_intent->>'quantity')::numeric<=0 OR (p_intent->>'limit_price')::numeric<=0 OR coalesce(p_intent->>'session_id','')='' THEN RAISE EXCEPTION 'invalid paper intent'; END IF;
    SELECT document INTO old_intent FROM execution.order_intents WHERE intent_id=intent_key;
    IF FOUND THEN
        IF (old_intent-'state'-'broker_order_id') IS DISTINCT FROM (p_intent-'state'-'broker_order_id') THEN RAISE EXCEPTION 'idempotency key reused with different intent'; END IF;
        RETURN false;
    END IF;
    IF account.recovered_generation IS DISTINCT FROM p_generation OR account.recovery_required
        OR account.reconciliation->>'state' IS DISTINCT FROM 'FLAT'
        OR jsonb_array_length(account.reconciliation#>'{snapshot,positions}')<>0
        OR account.latest_observed_at<clock_timestamp()-interval '60 seconds' THEN RAISE EXCEPTION 'paper account requires fresh flat reconciliation'; END IF;
    IF EXISTS (SELECT 1 FROM execution.order_intents WHERE account_id=p_account AND (session_id=p_intent->>'session_id' OR state<>'RESOLVED')) THEN RAISE EXCEPTION 'entry slot consumed or intent unresolved'; END IF;
    INSERT INTO execution.order_intents(intent_id,account_id,session_id,state,document)
        VALUES(intent_key,p_account,p_intent->>'session_id','PREPARED',p_intent);
    UPDATE execution.accounts SET recovery_required=true,recovered_generation=NULL WHERE account_id=p_account;
    INSERT INTO control.notification_outbox(event_key,payload,producer)
        VALUES('intent:'||intent_key,jsonb_build_object('event','PAPER_FIXTURE_INTENT','account_id',p_account,'intent_id',intent_key),session_user);
    INSERT INTO audit.events(actor,event_type,account_id,payload)
        VALUES(session_user,'PAPER_FIXTURE_INTENT',p_account,jsonb_build_object('intent_id',intent_key,'generation',p_generation));
    RETURN true;
END;
$$;

CREATE FUNCTION control.claim_notification(p_worker text,p_ttl integer)
RETURNS TABLE(event_key text,payload jsonb,claim_generation bigint)
LANGUAGE plpgsql SECURITY DEFINER SET search_path = pg_catalog, pg_temp AS $$
BEGIN
    IF p_ttl IS NULL OR p_ttl<1 OR p_ttl>300 OR p_worker IS NULL OR p_worker !~ '^[A-Za-z0-9_-]{1,96}$' THEN RAISE EXCEPTION 'invalid notification lease'; END IF;
    RETURN QUERY WITH picked AS (
        SELECT n.event_key FROM control.notification_outbox n WHERE n.delivered_at IS NULL AND (n.claim_until IS NULL OR n.claim_until<=clock_timestamp())
        ORDER BY n.created_at,n.event_key FOR UPDATE SKIP LOCKED LIMIT 1
    ) UPDATE control.notification_outbox n SET claim_worker=p_worker,claim_until=clock_timestamp()+make_interval(secs=>p_ttl),claim_generation=n.claim_generation+1
      FROM picked WHERE n.event_key=picked.event_key RETURNING n.event_key,n.payload,n.claim_generation;
END;
$$;
CREATE FUNCTION control.ack_notification(p_worker text,p_key text,p_generation bigint)
RETURNS boolean LANGUAGE plpgsql SECURITY DEFINER SET search_path = pg_catalog, pg_temp AS $$
BEGIN
    UPDATE control.notification_outbox SET delivered_at=clock_timestamp()
        WHERE event_key=p_key AND claim_worker=p_worker AND claim_generation=p_generation AND claim_until>clock_timestamp() AND delivered_at IS NULL;
    RETURN FOUND;
END;
$$;

GRANT USAGE ON SCHEMA policy,research,execution,control,audit TO trady_analyst,trady_executor,trady_gateway;
GRANT SELECT ON policy.versions TO trady_analyst,trady_executor,trady_gateway;
GRANT SELECT,INSERT ON research.runs TO trady_analyst;
GRANT SELECT ON research.runs TO trady_executor;
GRANT SELECT ON execution.account_summary TO trady_analyst,trady_gateway;
GRANT SELECT ON execution.accounts,execution.order_intents,execution.account_snapshots,execution.account_leases TO trady_executor;
GRANT INSERT(job_id,payload) ON control.research_jobs TO trady_analyst;
GRANT SELECT ON control.research_jobs,control.owner_commands TO trady_executor;
GRANT INSERT(command_id,command_kind,owner_user_id,account_id,payload) ON control.owner_commands TO trady_gateway;
GRANT EXECUTE ON FUNCTION execution.register_paper_account(text,text),execution.acquire_lease(text,text,integer),execution.renew_lease(text,text,bigint,integer),execution.save_reconciliation(text,text,bigint,jsonb),execution.reserve_paper_intent(text,text,bigint,jsonb) TO trady_executor;
GRANT EXECUTE ON FUNCTION control.claim_notification(text,integer),control.ack_notification(text,text,bigint) TO trady_gateway;
-- Runtime identities have no financial table mutation, DDL, role membership or deletion grants.
