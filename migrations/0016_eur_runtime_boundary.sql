-- Fenced EUR records and Telegram outbox. No direct runtime table mutations.
CREATE TABLE target.executor_leases (
 account_id text PRIMARY KEY CHECK (account_id ~ '^(paper|shadow|replay)_[A-Za-z0-9_-]{1,89}$'),
 worker_id text NOT NULL, backend_pid integer NOT NULL,
 generation bigint NOT NULL CHECK (generation>0), expires_at timestamptz NOT NULL
);
REVOKE ALL ON target.executor_leases FROM PUBLIC;
ALTER TABLE target.records DROP CONSTRAINT records_account_id_check;
ALTER TABLE target.records ADD CONSTRAINT records_account_id_check
 CHECK (account_id ~ '^(paper|shadow|replay|live)_[A-Za-z0-9_-]{1,89}$');

CREATE FUNCTION target.acquire_executor_lease(p_account text,p_worker text,p_ttl integer)
RETURNS bigint LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog,pg_temp AS $$
DECLARE old target.executor_leases%ROWTYPE;
BEGIN
 IF p_account IS NULL OR p_account !~ '^(paper|shadow|replay)_[A-Za-z0-9_-]{1,89}$'
 OR p_worker IS NULL OR p_worker !~ '^[A-Za-z0-9_-]{1,96}$'
 OR p_ttl IS NULL OR p_ttl NOT BETWEEN 1 AND 3600 THEN RAISE EXCEPTION 'INVALID_TARGET_LEASE'; END IF;
 INSERT INTO target.executor_leases VALUES(p_account,p_worker,pg_backend_pid(),1,'epoch') ON CONFLICT DO NOTHING;
 SELECT * INTO STRICT old FROM target.executor_leases WHERE account_id=p_account FOR UPDATE;
 IF old.expires_at>clock_timestamp() THEN RAISE EXCEPTION 'TARGET_LEASE_HELD'; END IF;
 UPDATE target.executor_leases SET worker_id=p_worker,backend_pid=pg_backend_pid(),
 generation=old.generation+1,expires_at=clock_timestamp()+make_interval(secs=>p_ttl) WHERE account_id=p_account;
 RETURN old.generation+1;
END $$;
CREATE FUNCTION target.assert_executor_lease(p_account text,p_worker text,p_generation bigint)
RETURNS void LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog,pg_temp AS $$
BEGIN
 PERFORM 1 FROM target.executor_leases WHERE account_id=p_account AND worker_id=p_worker
 AND generation=p_generation AND backend_pid=pg_backend_pid() AND expires_at>clock_timestamp() FOR UPDATE;
 IF NOT FOUND THEN RAISE EXCEPTION 'TARGET_FENCE_INVALID'; END IF;
END $$;
CREATE FUNCTION target.renew_executor_lease(p_account text,p_worker text,p_generation bigint,p_ttl integer)
RETURNS void LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog,pg_temp AS $$
BEGIN
 IF p_ttl IS NULL OR p_ttl NOT BETWEEN 1 AND 3600 THEN RAISE EXCEPTION 'INVALID_TARGET_TTL'; END IF;
 PERFORM target.assert_executor_lease(p_account,p_worker,p_generation);
 UPDATE target.executor_leases SET expires_at=clock_timestamp()+make_interval(secs=>p_ttl) WHERE account_id=p_account;
END $$;
CREATE FUNCTION target.release_executor_lease(p_account text,p_worker text,p_generation bigint)
RETURNS void LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog,pg_temp AS $$
BEGIN
 PERFORM target.assert_executor_lease(p_account,p_worker,p_generation);
 UPDATE target.executor_leases SET expires_at='epoch' WHERE account_id=p_account;
END $$;

REVOKE ALL ON FUNCTION target.append_record(text,text,text,text,jsonb,text)
 FROM PUBLIC,trady_executor,trady_gateway,trady_analyst;
CREATE FUNCTION target.append_scoped_record(
 p_kind text,p_id text,p_owner text,p_account text,p_document jsonb,
 p_canonical text,p_hash text,p_worker text,p_generation bigint
) RETURNS boolean LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog,pg_temp AS $$
DECLARE inserted integer; old target.records%ROWTYPE;
BEGIN
 IF p_kind IS NULL OR p_kind !~ '^[a-z][a-z0-9_]{1,63}$'
 OR p_id IS NULL OR p_id !~ '^[A-Za-z0-9][A-Za-z0-9_.:-]{0,159}$'
 OR p_owner IS NULL OR p_owner !~ '^[A-Za-z0-9][A-Za-z0-9_.:-]{0,159}$'
 OR p_account IS NULL OR p_account !~ '^(paper|shadow|replay)_[A-Za-z0-9_-]{1,89}$'
 OR p_document IS NULL OR jsonb_typeof(p_document)<>'object'
 OR p_canonical IS NULL OR octet_length(p_canonical)>4000000 OR p_canonical::jsonb IS DISTINCT FROM p_document
 OR p_hash IS NULL OR p_hash<>encode(sha256(convert_to(p_canonical,'UTF8')),'hex')
 THEN RAISE EXCEPTION 'INVALID_TARGET_RECORD'; END IF;
 IF session_user='trady_executor' THEN
  PERFORM target.assert_executor_lease(p_account,p_worker,p_generation);
 ELSIF session_user='trady_analyst' THEN
  IF p_kind NOT IN ('universe','quote','evidence','selection','signal','provider_snapshot','instrument_registry')
  THEN RAISE EXCEPTION 'TARGET_WRITER_FORBIDDEN'; END IF;
 ELSE RAISE EXCEPTION 'TARGET_WRITER_FORBIDDEN'; END IF;
 INSERT INTO target.records(kind,record_id,owner_id,account_id,document,content_sha256)
 VALUES(p_kind,p_id,p_owner,p_account,p_document,p_hash) ON CONFLICT DO NOTHING;
 GET DIAGNOSTICS inserted=ROW_COUNT;
 IF inserted=1 THEN RETURN true; END IF;
 SELECT * INTO STRICT old FROM target.records WHERE kind=p_kind AND record_id=p_id;
 IF old.owner_id<>p_owner OR old.account_id<>p_account OR old.document IS DISTINCT FROM p_document
 OR old.content_sha256<>p_hash THEN RAISE EXCEPTION 'TARGET_RECORD_REBOUND'; END IF;
 RETURN false;
END $$;
CREATE FUNCTION target.read_record(p_kind text,p_id text,p_owner text,p_account text)
RETURNS jsonb LANGUAGE sql SECURITY DEFINER SET search_path=pg_catalog,pg_temp AS $$
 SELECT document FROM target.records WHERE kind=p_kind AND record_id=p_id AND owner_id=p_owner AND account_id=p_account;
$$;

CREATE TABLE target.telegram_outbox (
 deployment_id text NOT NULL,event_id text NOT NULL,notification_kind text NOT NULL,chat_id bigint NOT NULL CHECK(chat_id>0),
 owner_id text NOT NULL,account_id text NOT NULL,payload_hash text NOT NULL,payload jsonb NOT NULL,
 status text NOT NULL DEFAULT 'PENDING' CHECK(status IN ('PENDING','SENDING','SENT','UNKNOWN')),
 attempt_count integer NOT NULL DEFAULT 0,next_attempt_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 sent_at timestamptz,last_error text,delivery_token text,
 PRIMARY KEY(deployment_id,event_id,notification_kind,chat_id)
);
REVOKE ALL ON target.telegram_outbox FROM PUBLIC;
CREATE FUNCTION target.enqueue_telegram(
 p_deployment text,p_event text,p_kind text,p_chat bigint,p_owner text,p_account text,
 p_payload jsonb,p_canonical text,p_hash text,p_worker text,p_generation bigint
) RETURNS boolean LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog,pg_temp AS $$
DECLARE inserted integer; old target.telegram_outbox%ROWTYPE;
BEGIN
 PERFORM target.assert_executor_lease(p_account,p_worker,p_generation);
 IF p_deployment IS NULL OR p_deployment !~ '^[A-Za-z0-9_-]{1,96}$'
 OR p_event IS NULL OR p_event !~ '^[A-Za-z0-9][A-Za-z0-9_.:-]{0,159}$'
 OR p_kind IS NULL OR p_kind !~ '^[A-Z_]{1,64}$' OR p_chat IS NULL OR p_chat<=0
 OR p_owner IS NULL OR p_owner !~ '^[A-Za-z0-9][A-Za-z0-9_.:-]{0,159}$'
 OR p_payload IS NULL OR jsonb_typeof(p_payload)<>'object'
 OR p_canonical IS NULL OR octet_length(p_canonical)>32000 OR p_canonical::jsonb IS DISTINCT FROM p_payload
 OR p_hash IS NULL OR p_hash<>encode(sha256(convert_to(p_canonical,'UTF8')),'hex')
 THEN RAISE EXCEPTION 'INVALID_TARGET_TELEGRAM'; END IF;
 INSERT INTO target.telegram_outbox(deployment_id,event_id,notification_kind,chat_id,owner_id,account_id,payload,payload_hash)
 VALUES(p_deployment,p_event,p_kind,p_chat,p_owner,p_account,p_payload,p_hash) ON CONFLICT DO NOTHING;
 GET DIAGNOSTICS inserted=ROW_COUNT;
 IF inserted=1 THEN RETURN true; END IF;
 SELECT * INTO STRICT old FROM target.telegram_outbox WHERE deployment_id=p_deployment
 AND event_id=p_event AND notification_kind=p_kind AND chat_id=p_chat;
 IF old.payload IS DISTINCT FROM p_payload OR old.payload_hash<>p_hash OR old.owner_id<>p_owner OR old.account_id<>p_account
 THEN RAISE EXCEPTION 'TARGET_TELEGRAM_REBOUND'; END IF;
 RETURN false;
END $$;
-- Never reclaim a crashed SENDING delivery: Telegram has no client-idempotency
-- key, so a crash after dispatch must remain ambiguous until operator review.
CREATE FUNCTION target.claim_telegram(p_deployment text,p_chats bigint[],p_token text)
RETURNS jsonb LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog,pg_temp AS $$
DECLARE row target.telegram_outbox%ROWTYPE;
BEGIN
 IF p_token IS NULL OR p_token !~ '^[A-Za-z0-9_-]{1,96}$' OR p_chats IS NULL
 OR cardinality(p_chats) NOT BETWEEN 1 AND 32 THEN RAISE EXCEPTION 'INVALID_TARGET_DELIVERY_CLAIM'; END IF;
 SELECT * INTO row FROM target.telegram_outbox WHERE deployment_id=p_deployment AND chat_id=ANY(p_chats)
 AND status='PENDING' AND next_attempt_at<=clock_timestamp()
 ORDER BY next_attempt_at,event_id,chat_id FOR UPDATE SKIP LOCKED LIMIT 1;
 IF NOT FOUND THEN RETURN NULL; END IF;
 UPDATE target.telegram_outbox SET status='SENDING',attempt_count=attempt_count+1,delivery_token=p_token
 WHERE deployment_id=row.deployment_id AND event_id=row.event_id AND notification_kind=row.notification_kind AND chat_id=row.chat_id;
 RETURN jsonb_build_object('event_id',row.event_id,'notification_kind',row.notification_kind,'chat_id',row.chat_id,
 'payload',row.payload,'payload_hash',row.payload_hash);
END $$;
CREATE FUNCTION target.finish_telegram(p_deployment text,p_event text,p_kind text,p_chat bigint,p_token text,p_sent boolean)
RETURNS void LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog,pg_temp AS $$
BEGIN
 IF p_sent IS NULL THEN RAISE EXCEPTION 'INVALID_TARGET_DELIVERY_RESULT'; END IF;
 UPDATE target.telegram_outbox SET status=CASE WHEN p_sent THEN 'SENT' ELSE 'UNKNOWN' END,
 sent_at=CASE WHEN p_sent THEN clock_timestamp() END,last_error=CASE WHEN p_sent THEN NULL ELSE 'DELIVERY_OUTCOME_UNKNOWN' END
 WHERE deployment_id=p_deployment AND event_id=p_event AND notification_kind=p_kind AND chat_id=p_chat
 AND delivery_token=p_token AND status='SENDING';
 IF NOT FOUND THEN RAISE EXCEPTION 'TARGET_DELIVERY_FENCE_INVALID'; END IF;
END $$;

REVOKE ALL ON FUNCTION target.acquire_executor_lease(text,text,integer),target.assert_executor_lease(text,text,bigint),
 target.renew_executor_lease(text,text,bigint,integer),target.release_executor_lease(text,text,bigint),
 target.append_scoped_record(text,text,text,text,jsonb,text,text,text,bigint),target.read_record(text,text,text,text),
 target.enqueue_telegram(text,text,text,bigint,text,text,jsonb,text,text,text,bigint),
 target.claim_telegram(text,bigint[],text),target.finish_telegram(text,text,text,bigint,text,boolean) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION target.acquire_executor_lease(text,text,integer),target.assert_executor_lease(text,text,bigint),
 target.renew_executor_lease(text,text,bigint,integer),target.release_executor_lease(text,text,bigint),
 target.enqueue_telegram(text,text,text,bigint,text,text,jsonb,text,text,text,bigint) TO trady_executor;
GRANT EXECUTE ON FUNCTION target.append_scoped_record(text,text,text,text,jsonb,text,text,text,bigint),
 target.read_record(text,text,text,text) TO trady_executor,trady_analyst;
GRANT EXECUTE ON FUNCTION target.claim_telegram(text,bigint[],text),
 target.finish_telegram(text,text,text,bigint,text,boolean) TO trady_gateway;
