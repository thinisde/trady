-- Isolated from the original paper-fixture ledger. Only the executor can advance it.
CREATE TABLE execution.autonomous_accounts (
    account_id text PRIMARY KEY CHECK (account_id ~ '^(paper|shadow|live)_[A-Za-z0-9_-]+$'),
    config jsonb NOT NULL,
    document jsonb NOT NULL,
    revision bigint NOT NULL DEFAULT 0 CHECK (revision >= 0),
    worker_id text,
    generation bigint NOT NULL DEFAULT 0,
    lease_until timestamptz
);

CREATE FUNCTION execution.acquire_trader(p_config jsonb,p_worker text,p_ttl integer,p_initial jsonb)
RETURNS bigint LANGUAGE plpgsql SECURITY DEFINER SET search_path = pg_catalog, pg_temp AS $$
DECLARE a execution.autonomous_accounts%ROWTYPE; key text; next_generation bigint;
BEGIN
    key=p_config->>'account_id';
    IF p_ttl IS NULL OR p_ttl<1 OR p_ttl>300 OR p_worker IS NULL OR p_worker !~ '^[A-Za-z0-9_-]{1,96}$'
       OR p_config->>'mode' NOT IN ('paper','shadow','live')
       OR key !~ ('^'||(p_config->>'mode')||'_[A-Za-z0-9_-]+$')
       OR coalesce(p_config->>'owner_id','')='' OR coalesce(p_config->>'base_currency','') !~ '^[A-Z]{3}$'
       OR p_initial->>'revision' IS DISTINCT FROM '0' OR p_initial->>'position_state' IS DISTINCT FROM 'FLAT'
       OR p_initial->'plan' IS DISTINCT FROM 'null'::jsonb OR p_initial->'orders' IS DISTINCT FROM '[]'::jsonb
       THEN RAISE EXCEPTION 'invalid autonomous account/lease'; END IF;
    INSERT INTO execution.autonomous_accounts(account_id,config,document) VALUES(key,p_config,p_initial) ON CONFLICT DO NOTHING;
    SELECT * INTO a FROM execution.autonomous_accounts WHERE account_id=key FOR UPDATE;
    IF a.config->>'mode' IS DISTINCT FROM p_config->>'mode' OR a.config->>'base_currency' IS DISTINCT FROM p_config->>'base_currency'
       OR a.config->>'owner_id' IS DISTINCT FROM p_config->>'owner_id' THEN RAISE EXCEPTION 'autonomous account binding changed'; END IF;
    IF a.lease_until>clock_timestamp() THEN RAISE EXCEPTION 'autonomous executor lease is busy'; END IF;
    IF a.config IS DISTINCT FROM p_config AND a.document->'plan' IS DISTINCT FROM 'null'::jsonb
       THEN RAISE EXCEPTION 'governing configuration cannot change while a plan is active'; END IF;
    UPDATE execution.autonomous_accounts SET config=p_config,worker_id=p_worker,generation=generation+1,lease_until=clock_timestamp()+make_interval(secs=>p_ttl)
      WHERE account_id=key RETURNING generation INTO next_generation;
    INSERT INTO audit.events(actor,event_type,account_id,payload) VALUES(session_user,'TRADER_ACQUIRED',key,jsonb_build_object('generation',next_generation,'worker',p_worker));
    RETURN next_generation;
END;
$$;

CREATE FUNCTION execution.assert_trader(p_account text,p_worker text,p_generation bigint)
RETURNS void LANGUAGE plpgsql SECURITY DEFINER SET search_path = pg_catalog, pg_temp AS $$
DECLARE a execution.autonomous_accounts%ROWTYPE;
BEGIN
    SELECT * INTO a FROM execution.autonomous_accounts WHERE account_id=p_account FOR UPDATE;
    IF NOT FOUND OR p_worker IS NULL OR p_generation IS NULL OR a.lease_until IS NULL
       OR a.worker_id IS DISTINCT FROM p_worker OR a.generation IS DISTINCT FROM p_generation OR a.lease_until<=clock_timestamp()
       THEN RAISE EXCEPTION 'autonomous executor lost its fence'; END IF;
END;
$$;

CREATE FUNCTION execution.renew_trader(p_account text,p_worker text,p_generation bigint,p_ttl integer)
RETURNS void LANGUAGE plpgsql SECURITY DEFINER SET search_path = pg_catalog, pg_temp AS $$
BEGIN
    IF p_ttl IS NULL OR p_ttl<1 OR p_ttl>300 THEN RAISE EXCEPTION 'invalid autonomous lease TTL'; END IF;
    PERFORM execution.assert_trader(p_account,p_worker,p_generation);
    UPDATE execution.autonomous_accounts SET lease_until=clock_timestamp()+make_interval(secs=>p_ttl) WHERE account_id=p_account;
END;
$$;

CREATE FUNCTION execution.load_trader(p_account text,p_worker text,p_generation bigint)
RETURNS jsonb LANGUAGE plpgsql SECURITY DEFINER SET search_path = pg_catalog, pg_temp AS $$
DECLARE doc jsonb;
BEGIN
    PERFORM execution.assert_trader(p_account,p_worker,p_generation);
    SELECT document INTO doc FROM execution.autonomous_accounts WHERE account_id=p_account;
    RETURN doc;
END;
$$;

CREATE FUNCTION execution.save_trader(p_account text,p_worker text,p_generation bigint,p_expected bigint,p_document jsonb)
RETURNS void LANGUAGE plpgsql SECURITY DEFINER SET search_path = pg_catalog, pg_temp AS $$
DECLARE a execution.autonomous_accounts%ROWTYPE; item jsonb; prior jsonb; i integer;
BEGIN
    PERFORM execution.assert_trader(p_account,p_worker,p_generation);
    SELECT * INTO a FROM execution.autonomous_accounts WHERE account_id=p_account;
    IF jsonb_typeof(p_document) IS DISTINCT FROM 'object'
       OR NOT (p_document ?& ARRAY['revision','position_state','plan','orders','consumed_sessions','loss_paused_sessions','last_snapshot','recovery_reason'])
       OR (p_document-ARRAY['revision','position_state','plan','orders','consumed_sessions','loss_paused_sessions','last_snapshot','recovery_reason'])<>'{}'::jsonb
       OR a.revision IS DISTINCT FROM p_expected OR (p_document->>'revision')::bigint IS DISTINCT FROM p_expected+1
       OR coalesce(p_document->>'position_state','') NOT IN ('FLAT','ENTRY_PENDING','OPEN_UNPROTECTED','OPEN_PROTECTED','EXIT_LATCHED','EXIT_PENDING','CLOSED','RECOVERY_REQUIRED')
       OR jsonb_typeof(p_document->'orders') IS DISTINCT FROM 'array'
       OR jsonb_typeof(p_document->'consumed_sessions') IS DISTINCT FROM 'array'
       OR jsonb_typeof(p_document->'loss_paused_sessions') IS DISTINCT FROM 'array'
       OR jsonb_array_length(p_document->'orders')<jsonb_array_length(a.document->'orders')
       OR NOT ((p_document->'consumed_sessions') @> (a.document->'consumed_sessions'))
       OR NOT ((p_document->'loss_paused_sessions') @> (a.document->'loss_paused_sessions'))
       THEN RAISE EXCEPTION 'invalid or stale autonomous transition'; END IF;
    IF p_document->'last_snapshot' IS DISTINCT FROM 'null'::jsonb AND
       (p_document#>>'{last_snapshot,account_id}' IS DISTINCT FROM p_account
        OR p_document#>>'{last_snapshot,mode}' IS DISTINCT FROM a.config->>'mode'
        OR p_document#>>'{last_snapshot,spendable_cash,currency}' IS DISTINCT FROM a.config->>'base_currency')
       THEN RAISE EXCEPTION 'autonomous observation account mismatch'; END IF;
    IF p_document->'plan' IS DISTINCT FROM 'null'::jsonb AND
       (p_document#>>'{plan,candidate,account_id}' IS DISTINCT FROM p_account
        OR p_document#>>'{plan,candidate,policy_version}' IS DISTINCT FROM a.config#>>'{policy,version}')
       THEN RAISE EXCEPTION 'autonomous plan scope mismatch'; END IF;
    IF a.document#>>'{plan,candidate,proposal_id}' IS NOT NULL
       AND a.document#>>'{plan,candidate,proposal_id}'=p_document#>>'{plan,candidate,proposal_id}'
       AND a.document#>>'{plan,exit_latch}' IS NOT NULL
       AND p_document#>>'{plan,exit_latch}' IS DISTINCT FROM a.document#>>'{plan,exit_latch}'
       THEN RAISE EXCEPTION 'exit latch cannot be reversed'; END IF;
    FOR i IN 0..jsonb_array_length(p_document->'orders')-1 LOOP
       item=p_document->'orders'->i; prior=a.document->'orders'->i;
       IF item#>>'{request,account_id}' IS DISTINCT FROM p_account
          OR (item#>>'{request,quantity}')::numeric<=0
          OR item->>'role' NOT IN ('ENTRY','PROTECTION','EXIT')
          OR (item->>'role'='ENTRY' AND item#>>'{request,side}' IS DISTINCT FROM 'BUY')
          OR (item->>'role'<>'ENTRY' AND item#>>'{request,side}' IS DISTINCT FROM 'SELL')
          OR (a.config->>'mode'='shadow' AND (item->>'attempted')::boolean)
          OR (prior IS NOT NULL AND (prior->'request' IS DISTINCT FROM item->'request' OR prior->'role' IS DISTINCT FROM item->'role'))
          OR (prior->>'attempted'='true' AND item->>'attempted' IS DISTINCT FROM 'true')
          THEN RAISE EXCEPTION 'immutable intent or executor boundary violated'; END IF;
       IF prior IS NULL AND item->>'role'='ENTRY' AND
          (p_document->'plan' IS NOT DISTINCT FROM 'null'::jsonb
           OR coalesce(p_document#>>'{plan,entry_session}','')=''
           OR NOT (p_document->'consumed_sessions' ? (p_document#>>'{plan,entry_session}'))
           OR a.document->'consumed_sessions' ? (p_document#>>'{plan,entry_session}'))
          THEN RAISE EXCEPTION 'autonomous daily entry slot is missing or consumed'; END IF;
    END LOOP;
    UPDATE execution.autonomous_accounts SET document=p_document,revision=p_expected+1 WHERE account_id=p_account;
    INSERT INTO audit.events(actor,event_type,account_id,payload) VALUES(session_user,'TRADER_TRANSITION',p_account,jsonb_build_object('generation',p_generation,'revision',p_expected+1,'state',p_document->>'position_state'));
    INSERT INTO control.notification_outbox(event_key,payload,producer) VALUES('trader:'||p_account||':'||(p_expected+1),jsonb_build_object('event','TRADER_TRANSITION','account_id',p_account,'state',p_document->>'position_state','revision',p_expected+1),session_user);
END;
$$;

CREATE FUNCTION execution.release_trader(p_account text,p_worker text,p_generation bigint)
RETURNS void LANGUAGE plpgsql SECURITY DEFINER SET search_path = pg_catalog, pg_temp AS $$
BEGIN
    PERFORM execution.assert_trader(p_account,p_worker,p_generation);
    UPDATE execution.autonomous_accounts SET lease_until=clock_timestamp() WHERE account_id=p_account;
END;
$$;

REVOKE ALL ON execution.autonomous_accounts FROM PUBLIC,trady_analyst,trady_gateway,trady_executor;
REVOKE ALL ON FUNCTION execution.acquire_trader(jsonb,text,integer,jsonb),execution.assert_trader(text,text,bigint),execution.renew_trader(text,text,bigint,integer),execution.load_trader(text,text,bigint),execution.save_trader(text,text,bigint,bigint,jsonb),execution.release_trader(text,text,bigint) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION execution.acquire_trader(jsonb,text,integer,jsonb),execution.renew_trader(text,text,bigint,integer),execution.load_trader(text,text,bigint),execution.save_trader(text,text,bigint,bigint,jsonb),execution.release_trader(text,text,bigint) TO trady_executor;

CREATE TABLE research.trading_records (
    owner_id text NOT NULL,
    account_id text NOT NULL,
    kind text NOT NULL CHECK (kind IN ('screening','dossier','evidence','calculation','context','assessment','review')),
    record_id text NOT NULL,
    document jsonb NOT NULL,
    retain_until timestamptz NOT NULL,
    PRIMARY KEY(owner_id,account_id,kind,record_id)
);
CREATE FUNCTION research.put_trading_record(p_owner text,p_account text,p_kind text,p_id text,p_document jsonb,p_retain timestamptz)
RETURNS void LANGUAGE plpgsql SECURITY DEFINER SET search_path = pg_catalog, pg_temp AS $$
DECLARE old research.trading_records%ROWTYPE;
BEGIN
    IF coalesce(p_owner,'')='' OR coalesce(p_account,'')='' OR coalesce(p_id,'')='' OR p_retain<=clock_timestamp()
       OR jsonb_typeof(p_document) IS DISTINCT FROM 'object' OR octet_length(p_document::text)>8388608
       THEN RAISE EXCEPTION 'invalid trading research record'; END IF;
    PERFORM pg_advisory_xact_lock(hashtextextended(p_owner||':'||p_account||':'||p_kind||':'||p_id,0));
    SELECT * INTO old FROM research.trading_records WHERE owner_id=p_owner AND account_id=p_account AND kind=p_kind AND record_id=p_id;
    IF FOUND THEN
       IF old.document IS DISTINCT FROM p_document THEN RAISE EXCEPTION 'research record identity is immutable'; END IF;
       IF p_retain<old.retain_until THEN
          UPDATE research.trading_records SET retain_until=p_retain WHERE owner_id=p_owner AND account_id=p_account AND kind=p_kind AND record_id=p_id;
       END IF;
       RETURN;
    END IF;
    INSERT INTO research.trading_records VALUES(p_owner,p_account,p_kind,p_id,p_document,p_retain);
END;
$$;
CREATE FUNCTION research.purge_trading_records(p_owner text,p_account text)
RETURNS void LANGUAGE plpgsql SECURITY DEFINER SET search_path = pg_catalog, pg_temp AS $$
BEGIN
    DELETE FROM research.trading_records WHERE owner_id=p_owner AND account_id=p_account AND retain_until<=clock_timestamp();
END;
$$;
REVOKE ALL ON research.trading_records FROM PUBLIC,trady_executor,trady_gateway,trady_analyst;
GRANT SELECT ON research.trading_records TO trady_analyst;
REVOKE ALL ON FUNCTION research.put_trading_record(text,text,text,text,jsonb,timestamptz),research.purge_trading_records(text,text) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION research.put_trading_record(text,text,text,text,jsonb,timestamptz),research.purge_trading_records(text,text) TO trady_analyst;
