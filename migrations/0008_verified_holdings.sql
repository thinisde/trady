-- The executor attests actual holdings. Analysts can resolve advice contexts but
-- cannot insert a position, change a plan, move a stop, or grant order authority.
CREATE TABLE execution.holding_observations (
    account_id text PRIMARY KEY REFERENCES execution.autonomous_accounts(account_id),
    observed_at timestamptz NOT NULL,
    document jsonb NOT NULL
);

CREATE FUNCTION execution.record_holding_observation(p_account text,p_worker text,p_generation bigint,p_document jsonb)
RETURNS void LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog,pg_temp AS $$
DECLARE a execution.autonomous_accounts%ROWTYPE; prior execution.holding_observations%ROWTYPE; stamp timestamptz;
BEGIN
    PERFORM execution.assert_trader(p_account,p_worker,p_generation);
    SELECT * INTO a FROM execution.autonomous_accounts WHERE account_id=p_account;
    stamp=(p_document#>>'{observation,snapshot,observed_at}')::timestamptz;
    IF jsonb_typeof(p_document) IS DISTINCT FROM 'object'
       OR (p_document-ARRAY['observation','exit_costs'])<>'{}'::jsonb
       OR NOT (p_document ?& ARRAY['observation','exit_costs'])
       OR octet_length(p_document::text)>8388608
       OR p_document#>>'{observation,snapshot,account_id}' IS DISTINCT FROM p_account
       OR p_document#>>'{observation,snapshot,mode}' IS DISTINCT FROM a.config->>'mode'
       OR p_document#>>'{observation,snapshot,spendable_cash,currency}' IS DISTINCT FROM a.config->>'base_currency'
       OR p_document#>'{observation,snapshot}' IS DISTINCT FROM a.document->'last_snapshot'
       OR jsonb_typeof(p_document#>'{observation,position_costs}') IS DISTINCT FROM 'object'
       OR jsonb_typeof(p_document#>'{exit_costs}') IS DISTINCT FROM 'object'
       OR stamp IS NULL OR stamp>clock_timestamp() OR stamp<=clock_timestamp()-interval '60 seconds'
       THEN RAISE EXCEPTION 'invalid executor holding observation'; END IF;
    SELECT * INTO prior FROM execution.holding_observations WHERE account_id=p_account;
    IF FOUND AND (prior.observed_at>stamp OR (prior.observed_at=stamp AND prior.document IS DISTINCT FROM p_document))
       THEN RAISE EXCEPTION 'holding observation regressed or changed identity'; END IF;
    INSERT INTO execution.holding_observations VALUES(p_account,stamp,p_document)
      ON CONFLICT(account_id) DO UPDATE SET observed_at=EXCLUDED.observed_at,document=EXCLUDED.document;
END;
$$;

CREATE FUNCTION research.verified_holding(p_owner text,p_account text,p_position text)
RETURNS jsonb LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog,pg_temp AS $$
DECLARE a execution.autonomous_accounts%ROWTYPE; o execution.holding_observations%ROWTYPE;
BEGIN
    SELECT * INTO a FROM execution.autonomous_accounts WHERE account_id=p_account;
    IF NOT FOUND OR a.config->>'owner_id' IS DISTINCT FROM p_owner
       OR a.document#>>'{plan,candidate,proposal_id}' IS DISTINCT FROM p_position
       OR a.document->>'position_state' NOT IN ('OPEN_PROTECTED','OPEN_UNPROTECTED')
       OR a.document#>'{plan,exit_latch}' IS DISTINCT FROM 'null'::jsonb
       OR a.document#>'{plan,holding_state,exit_latch}' IS DISTINCT FROM 'null'::jsonb
       THEN RAISE EXCEPTION 'verified active holding is unavailable'; END IF;
    SELECT * INTO o FROM execution.holding_observations WHERE account_id=p_account;
    IF NOT FOUND OR o.observed_at<=clock_timestamp()-interval '60 seconds'
       OR o.document#>'{observation,snapshot}' IS DISTINCT FROM a.document->'last_snapshot'
       THEN RAISE EXCEPTION 'verified holding observation is stale'; END IF;
    RETURN jsonb_build_object('owner_id',p_owner,'account_id',p_account,'position_id',p_position,
        'revision',a.revision,'policy',a.config->'policy','plan',a.document->'plan',
        'observation',o.document->'observation','exit_costs',o.document->'exit_costs');
END;
$$;

REVOKE ALL ON execution.holding_observations FROM PUBLIC,trady_analyst,trady_gateway,trady_executor;
REVOKE ALL ON FUNCTION execution.record_holding_observation(text,text,bigint,jsonb),research.verified_holding(text,text,text) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION execution.record_holding_observation(text,text,bigint,jsonb) TO trady_executor;
GRANT EXECUTE ON FUNCTION research.verified_holding(text,text,text) TO trady_analyst;

CREATE TABLE research.holding_updates (
    owner_id text NOT NULL,
    account_id text NOT NULL,
    update_id text NOT NULL,
    position_id text NOT NULL,
    expected_version bigint NOT NULL CHECK(expected_version>0),
    valid_until timestamptz NOT NULL,
    retain_until timestamptz NOT NULL,
    document jsonb NOT NULL,
    PRIMARY KEY(owner_id,account_id,update_id)
);

-- Hash-only identities survive deletion of retained source/context copies.
CREATE TABLE research.holding_update_identities (
    owner_id text NOT NULL,account_id text NOT NULL,update_id text NOT NULL,
    document_hash text NOT NULL CHECK(document_hash ~ '^[a-f0-9]{64}$'),
    PRIMARY KEY(owner_id,account_id,update_id)
);

CREATE FUNCTION research.queue_holding_update(p_owner text,p_account text,p_document jsonb)
RETURNS void LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog,pg_temp AS $$
DECLARE a execution.autonomous_accounts%ROWTYPE; prior text; assessment jsonb; expiry timestamptz;
    assessment_retention timestamptz; review_retention timestamptz; dossier_retention timestamptz; retention timestamptz;
BEGIN
    SELECT * INTO a FROM execution.autonomous_accounts WHERE account_id=p_account;
    expiry=(p_document->>'valid_until')::timestamptz;
    IF NOT FOUND OR a.config->>'owner_id' IS DISTINCT FROM p_owner
       OR a.document#>>'{plan,candidate,proposal_id}' IS DISTINCT FROM p_document->>'position_id'
       OR a.document#>>'{plan,holding_state,plan,version}' IS DISTINCT FROM p_document->>'expected_plan_version'
       OR a.document#>'{plan,exit_latch}' IS DISTINCT FROM 'null'::jsonb
       OR jsonb_typeof(p_document) IS DISTINCT FROM 'object'
       OR coalesce(p_document->>'id','') !~ '^[A-Za-z0-9_-]{1,128}$'
       OR expiry IS NULL OR expiry<=clock_timestamp() OR expiry>clock_timestamp()+interval '5 minutes'
       OR octet_length(p_document::text)>8388608
       THEN RAISE EXCEPTION 'invalid scoped holding update'; END IF;
    SELECT document,retain_until INTO assessment,assessment_retention FROM research.trading_records
      WHERE owner_id=p_owner AND account_id=p_account AND kind='assessment'
        AND record_id=p_document#>>'{assessment,assessment,assessment_id}' AND retain_until>clock_timestamp()
      FOR UPDATE;
    IF NOT FOUND OR assessment IS DISTINCT FROM p_document->'assessment'
       OR assessment#>>'{assessment,status}' IS DISTINCT FROM 'OK'
       OR (assessment#>>'{assessment,valid_until}')::timestamptz<expiry
       OR assessment#>>'{context,scenario,binding,arguments,position_id}' IS DISTINCT FROM p_document->>'position_id'
       THEN RAISE EXCEPTION 'holding update does not resolve persisted native review and research'; END IF;
    SELECT retain_until INTO dossier_retention FROM research.trading_records
      WHERE owner_id=p_owner AND account_id=p_account AND kind='dossier'
        AND record_id=p_document#>>'{research,dossier,id}'
        AND document=p_document#>'{research,dossier}' AND retain_until>clock_timestamp()
      FOR UPDATE;
    SELECT min(locked.retain_until) INTO review_retention FROM (
      SELECT retain_until FROM research.trading_records WHERE owner_id=p_owner AND account_id=p_account
        AND kind='review' AND document->'review'=p_document->'review'
        AND document->'scenario'=assessment#>'{context,scenario,binding}' AND retain_until>clock_timestamp()
      ORDER BY record_id FOR UPDATE
    ) AS locked;
    IF dossier_retention IS NULL OR review_retention IS NULL
       THEN RAISE EXCEPTION 'holding update does not resolve persisted native review and research'; END IF;
    PERFORM pg_advisory_xact_lock(hashtextextended(p_owner||':'||p_account||':'||(p_document->>'id'),0));
    SELECT document_hash INTO prior FROM research.holding_update_identities WHERE owner_id=p_owner AND account_id=p_account AND update_id=p_document->>'id';
    IF FOUND THEN
       IF prior IS DISTINCT FROM encode(sha256(convert_to(p_document::text,'UTF8')),'hex') THEN RAISE EXCEPTION 'holding update identity is immutable'; END IF;
       RETURN;
    END IF;
    retention=least(expiry,assessment_retention,review_retention,dossier_retention);
    -- Publish an MVCC row version even though retention is unchanged. A writer
    -- with an older REPEATABLE READ snapshot must serialize/retry instead of
    -- shortening a source while its trigger cannot see this new queued copy.
    UPDATE research.trading_records SET retain_until=retain_until
      WHERE owner_id=p_owner AND account_id=p_account AND (
        (kind='assessment' AND record_id=p_document#>>'{assessment,assessment,assessment_id}')
        OR (kind='dossier' AND record_id=p_document#>>'{research,dossier,id}')
        OR (kind='review' AND document->'review'=p_document->'review'
          AND document->'scenario'=assessment#>'{context,scenario,binding}'));
    IF retention<=clock_timestamp() THEN RAISE EXCEPTION 'holding update source retention expired'; END IF;
    INSERT INTO research.holding_update_identities VALUES(p_owner,p_account,p_document->>'id',encode(sha256(convert_to(p_document::text,'UTF8')),'hex'));
    INSERT INTO research.holding_updates VALUES(p_owner,p_account,p_document->>'id',p_document->>'position_id',
      (p_document->>'expected_plan_version')::bigint,expiry,retention,p_document);
END;
$$;

-- Later shortening or removal of a source also applies to every queued copy.
CREATE FUNCTION research.refresh_holding_update_retention()
RETURNS trigger LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog,pg_temp AS $$
BEGIN
    IF TG_OP='UPDATE' THEN
      IF NEW.retain_until>=OLD.retain_until THEN RETURN NULL; END IF;
    END IF;
    IF TG_OP='DELETE' THEN
      DELETE FROM research.holding_updates AS queued
      WHERE queued.owner_id=OLD.owner_id AND queued.account_id=OLD.account_id AND (
        (OLD.kind='assessment' AND queued.document#>>'{assessment,assessment,assessment_id}'=OLD.record_id)
        OR (OLD.kind='dossier' AND queued.document#>>'{research,dossier,id}'=OLD.record_id)
        OR (OLD.kind='review' AND queued.document->'review'=OLD.document->'review'
          AND queued.document#>'{assessment,context,scenario,binding}'=OLD.document->'scenario'));
    ELSE
      UPDATE research.holding_updates AS queued SET retain_until=least(queued.retain_until,NEW.retain_until)
      WHERE queued.owner_id=OLD.owner_id AND queued.account_id=OLD.account_id AND (
        (OLD.kind='assessment' AND queued.document#>>'{assessment,assessment,assessment_id}'=OLD.record_id)
        OR (OLD.kind='dossier' AND queued.document#>>'{research,dossier,id}'=OLD.record_id)
        OR (OLD.kind='review' AND queued.document->'review'=OLD.document->'review'
          AND queued.document#>'{assessment,context,scenario,binding}'=OLD.document->'scenario'));
    END IF;
    RETURN NULL;
END;
$$;
CREATE TRIGGER holding_update_source_retention AFTER DELETE OR UPDATE OF retain_until ON research.trading_records
FOR EACH ROW EXECUTE FUNCTION research.refresh_holding_update_retention();
REVOKE ALL ON FUNCTION research.refresh_holding_update_retention() FROM PUBLIC;

CREATE FUNCTION execution.pending_holding_update(p_account text,p_worker text,p_generation bigint)
RETURNS jsonb LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog,pg_temp AS $$
DECLARE a execution.autonomous_accounts%ROWTYPE; result jsonb;
BEGIN
    PERFORM execution.assert_trader(p_account,p_worker,p_generation);
    SELECT * INTO a FROM execution.autonomous_accounts WHERE account_id=p_account;
    SELECT document INTO result FROM research.holding_updates WHERE owner_id=a.config->>'owner_id'
      AND account_id=p_account AND position_id=a.document#>>'{plan,candidate,proposal_id}'
      AND expected_version=(a.document#>>'{plan,holding_state,plan,version}')::bigint
      AND valid_until>clock_timestamp() AND retain_until>clock_timestamp() ORDER BY valid_until DESC,update_id LIMIT 1;
    RETURN result;
END;
$$;
REVOKE ALL ON research.holding_updates FROM PUBLIC,trady_analyst,trady_gateway,trady_executor;
REVOKE ALL ON research.holding_update_identities FROM PUBLIC,trady_analyst,trady_gateway,trady_executor;
REVOKE ALL ON FUNCTION research.queue_holding_update(text,text,jsonb),execution.pending_holding_update(text,text,bigint) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION research.queue_holding_update(text,text,jsonb) TO trady_analyst;
GRANT EXECUTE ON FUNCTION execution.pending_holding_update(text,text,bigint) TO trady_executor;

CREATE OR REPLACE FUNCTION research.purge_trading_records(p_owner text,p_account text)
RETURNS void LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog,pg_temp AS $$
BEGIN
    DELETE FROM research.holding_updates WHERE owner_id=p_owner AND account_id=p_account AND retain_until<=clock_timestamp();
    DELETE FROM research.trading_records WHERE owner_id=p_owner AND account_id=p_account AND retain_until<=clock_timestamp();
END;
$$;

CREATE TABLE execution.control_latches (
    account_id text PRIMARY KEY REFERENCES execution.autonomous_accounts(account_id),
    pause_entries boolean NOT NULL,
    close_position boolean NOT NULL,
    revision bigint NOT NULL DEFAULT 0,
    updated_at timestamptz NOT NULL DEFAULT clock_timestamp()
);

CREATE FUNCTION execution.synchronize_owner_controls(p_account text,p_worker text,p_generation bigint,
    p_context jsonb,p_hash text,p_defaults jsonb)
RETURNS jsonb LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog,pg_temp AS $$
DECLARE a execution.autonomous_accounts%ROWTYPE; l execution.control_latches%ROWTYPE;
    command control.owner_controls%ROWTYPE; accepted boolean; stamp timestamptz;
BEGIN
    PERFORM execution.assert_trader(p_account,p_worker,p_generation);
    SELECT * INTO a FROM execution.autonomous_accounts WHERE account_id=p_account;
    stamp=(p_context->>'observed_at')::timestamptz;
    IF p_context#>>'{scope,account_id}' IS DISTINCT FROM p_account
       OR p_context#>>'{scope,owner_id}' IS DISTINCT FROM a.config->>'owner_id'
       OR p_context#>>'{scope,policy_version}' IS DISTINCT FROM a.config#>>'{policy,version}'
       OR p_context->>'mode' IS DISTINCT FROM a.config->>'mode'
       OR p_context->>'observed_at' IS DISTINCT FROM a.document#>>'{last_snapshot,observed_at}'
       OR stamp IS NULL OR stamp>clock_timestamp() OR stamp<=clock_timestamp()-interval '60 seconds'
       OR p_hash !~ '^[a-f0-9]{64}$' OR jsonb_typeof(p_defaults) IS DISTINCT FROM 'object'
       OR NOT(p_defaults ?& ARRAY['pause_entries','close_position'])
       OR jsonb_typeof(p_defaults->'pause_entries') IS DISTINCT FROM 'boolean'
       OR jsonb_typeof(p_defaults->'close_position') IS DISTINCT FROM 'boolean'
       THEN RAISE EXCEPTION 'owner controls require the current reconciled executor context'; END IF;
    INSERT INTO control.owner_control_context(account_id,owner_id,policy_version,document,context_hash,observed_at)
      VALUES(p_account,a.config->>'owner_id',(a.config#>>'{policy,version}')::integer,p_context,p_hash,stamp)
      ON CONFLICT(account_id) DO UPDATE SET owner_id=EXCLUDED.owner_id,policy_version=EXCLUDED.policy_version,
        document=EXCLUDED.document,context_hash=EXCLUDED.context_hash,observed_at=EXCLUDED.observed_at;
    INSERT INTO execution.control_latches(account_id,pause_entries,close_position)
      VALUES(p_account,(p_defaults->>'pause_entries')::boolean,(p_defaults->>'close_position')::boolean)
      ON CONFLICT DO NOTHING;
    SELECT * INTO l FROM execution.control_latches WHERE account_id=p_account FOR UPDATE;
    l.pause_entries=l.pause_entries OR (p_defaults->>'pause_entries')::boolean
      OR (p_defaults->>'close_position')::boolean;
    l.close_position=l.close_position OR (p_defaults->>'close_position')::boolean;
    FOR command IN SELECT * FROM control.owner_controls WHERE account_id=p_account AND status='REQUESTED'
       AND producer='trady_gateway' ORDER BY created_at,command_id LIMIT 100 FOR UPDATE LOOP
        accepted=command.owner_id=a.config->>'owner_id'
          AND command.policy_version=(a.config#>>'{policy,version}')::integer
          AND command.chat_id=command.owner_user_id
          AND command.payload#>>'{scope,owner_id}'=command.owner_id
          AND command.payload#>>'{scope,account_id}'=p_account
          AND command.payload->>'id'=command.command_id
          AND command.payload->>'context_hash'=command.context_hash;
        IF command.kind<>'PAUSE_ENTRIES' THEN
            accepted=accepted AND command.confirmed_at IS NOT NULL AND command.expires_at>clock_timestamp()
              AND command.context_hash=p_hash;
        END IF;
        IF command.kind='RESUME_ENTRIES' AND l.close_position AND (
            p_context->>'instrument_id' IS NOT NULL OR a.document->'plan'<>'null'::jsonb
            OR EXISTS(SELECT 1 FROM jsonb_array_elements(a.document#>'{last_snapshot,orders}') o
                WHERE coalesce(o->>'status','UNKNOWN') NOT IN ('FILLED','CANCELLED','REJECTED'))) THEN
            accepted=false;
        END IF;
        IF command.kind='RESUME_ENTRIES' AND ((p_defaults->>'pause_entries')::boolean
            OR (p_defaults->>'close_position')::boolean) THEN
            accepted=false;
        END IF;
        IF NOT coalesce(accepted,false) THEN
            UPDATE control.owner_controls SET status='REJECTED',result=jsonb_build_object('reason','scope, confirmation or reconciled context changed')
              WHERE command_id=command.command_id;
            CONTINUE;
        END IF;
        IF command.kind='PAUSE_ENTRIES' THEN l.pause_entries=true;
        ELSIF command.kind='RESUME_ENTRIES' THEN l.pause_entries=false;l.close_position=false;
        ELSIF command.kind='CLOSE_POSITION' THEN l.pause_entries=true;l.close_position=true;
        ELSE RAISE EXCEPTION 'unsupported durable owner control'; END IF;
        l.revision=l.revision+1;
        UPDATE control.owner_controls SET status=CASE WHEN command.kind='CLOSE_POSITION' THEN 'ACCEPTED' ELSE 'COMPLETE' END,
          result=jsonb_build_object('pause_entries',l.pause_entries,'close_position',l.close_position,
             'controls_revision',l.revision,'account_observed_at',stamp) WHERE command_id=command.command_id;
        INSERT INTO audit.events(actor,event_type,account_id,payload) VALUES(session_user,'OWNER_CONTROL_APPLIED',p_account,
          jsonb_build_object('command_id',command.command_id,'kind',command.kind,'revision',l.revision));
    END LOOP;
    IF p_context->>'instrument_id' IS NULL AND a.document->'plan'='null'::jsonb
       AND NOT EXISTS(SELECT 1 FROM jsonb_array_elements(a.document#>'{last_snapshot,orders}') o
           WHERE coalesce(o->>'status','UNKNOWN') NOT IN ('FILLED','CANCELLED','REJECTED')) THEN
        UPDATE control.owner_controls SET status='COMPLETE',result=jsonb_build_object('flat_observed_at',stamp)
          WHERE account_id=p_account AND owner_id=a.config->>'owner_id' AND kind='CLOSE_POSITION' AND status IN ('ACCEPTED','SUBMITTED','PARTIAL');
    END IF;
    UPDATE execution.control_latches SET pause_entries=l.pause_entries,close_position=l.close_position,
      revision=l.revision,updated_at=clock_timestamp() WHERE account_id=p_account;
    RETURN jsonb_build_object('pause_entries',l.pause_entries,'close_position',l.close_position);
END;
$$;
REVOKE ALL ON execution.control_latches FROM PUBLIC,trady_analyst,trady_gateway,trady_executor;
REVOKE ALL ON FUNCTION execution.synchronize_owner_controls(text,text,bigint,jsonb,text,jsonb) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION execution.synchronize_owner_controls(text,text,bigint,jsonb,text,jsonb) TO trady_executor;

-- Dependency validation and plan acceptance share the executor transaction.
-- Risk-reducing reconciliation/protection never depends on expiring research.
ALTER FUNCTION execution.save_trader(text,text,bigint,bigint,jsonb) RENAME TO save_trader_core;
REVOKE ALL ON FUNCTION execution.save_trader_core(text,text,bigint,bigint,jsonb) FROM PUBLIC,trady_executor;
CREATE FUNCTION execution.save_trader(p_account text,p_worker text,p_generation bigint,p_expected bigint,p_document jsonb)
RETURNS void LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog,pg_temp AS $$
DECLARE a execution.autonomous_accounts%ROWTYPE; reference jsonb; activation boolean; item jsonb; ordinal bigint;
BEGIN
    PERFORM execution.assert_trader(p_account,p_worker,p_generation);
    SELECT * INTO a FROM execution.autonomous_accounts WHERE account_id=p_account;
    activation=p_document#>>'{plan,candidate,proposal_id}' IS NOT NULL AND (
       a.document#>>'{plan,candidate,proposal_id}' IS DISTINCT FROM p_document#>>'{plan,candidate,proposal_id}'
       OR (p_document#>>'{plan,holding_state,plan,version}')::bigint>(a.document#>>'{plan,holding_state,plan,version}')::bigint);
    FOR item,ordinal IN SELECT value,ordinality FROM jsonb_array_elements(p_document->'orders') WITH ORDINALITY LOOP
       IF item->>'role'='ENTRY' AND item->>'attempted'='true'
          AND (a.document->'orders'->((ordinal-1)::integer)->>'attempted') IS DISTINCT FROM 'true' THEN activation=true; END IF;
    END LOOP;
    IF activation THEN
       reference=p_document#>'{plan,candidate,sentiment_reference}';
       IF a.config->>'mode'='live' AND (reference IS NULL OR reference='null'::jsonb) THEN
          RAISE EXCEPTION 'live activation requires persisted sentiment dependencies'; END IF;
       IF reference IS NOT NULL AND reference<>'null'::jsonb AND (
          reference#>>'{scope,owner_id}' IS DISTINCT FROM a.config->>'owner_id'
          OR reference#>>'{scope,account_id}' IS DISTINCT FROM p_account
          OR reference#>>'{scope,policy_version}' IS DISTINCT FROM a.config#>>'{policy,version}'
          OR reference->>'instrument_id' IS DISTINCT FROM p_document#>>'{plan,candidate,instrument_id}'
          OR NOT research.sentiment_reference_current(reference)) THEN
          RAISE EXCEPTION 'sentiment dependency changed before plan acceptance'; END IF;
    END IF;
    PERFORM execution.save_trader_core(p_account,p_worker,p_generation,p_expected,p_document);
END;
$$;
REVOKE ALL ON FUNCTION execution.save_trader(text,text,bigint,bigint,jsonb) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION execution.save_trader(text,text,bigint,bigint,jsonb) TO trady_executor;
