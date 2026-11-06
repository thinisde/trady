-- Physical DEMO authority, shared across every logical scope and owner.
-- Historical reservations are retained, including failed/UNKNOWN attempts.
CREATE TABLE target.physical_entry_reservations (
 environment text NOT NULL DEFAULT 'DEMO' CHECK(environment='DEMO'),
 broker_account_id bigint NOT NULL DEFAULT 52211463 CHECK(broker_account_id=52211463),
 session_day date NOT NULL, owner_id text NOT NULL, account_id text NOT NULL,
 intent_id text NOT NULL, research_cycle_id text, reserved_at timestamptz NOT NULL,
 PRIMARY KEY(environment,broker_account_id,session_day,owner_id,account_id,intent_id)
);
CREATE UNIQUE INDEX physical_entry_cycle_once ON target.physical_entry_reservations
(environment,broker_account_id,research_cycle_id) WHERE research_cycle_id IS NOT NULL;
INSERT INTO target.physical_entry_reservations(session_day,owner_id,account_id,intent_id,reserved_at)
 SELECT session_day,owner_id,account_id,intent_id,reserved_at FROM target.daily_entry_slots WHERE account_id LIKE 'paper\_%' ESCAPE '\';
REVOKE ALL ON target.physical_entry_reservations FROM PUBLIC;

CREATE TABLE target.demo_physical_lease (
 broker_account_id bigint PRIMARY KEY CHECK(broker_account_id=52211463),
 account_id text NOT NULL,worker_id text NOT NULL,backend_pid integer NOT NULL,
 generation bigint NOT NULL,expires_at timestamptz NOT NULL
);
REVOKE ALL ON target.demo_physical_lease FROM PUBLIC;
CREATE FUNCTION target.acquire_demo_executor_lease(p_account text,p_worker text,p_ttl integer)
RETURNS bigint LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog,pg_temp AS $$
DECLARE held boolean:=false; prior target.demo_physical_lease%ROWTYPE; gen bigint;
BEGIN
 IF session_user<>'trady_executor' OR p_account IS NULL OR p_account !~ '^paper_[A-Za-z0-9_-]{1,89}$'
 THEN RAISE EXCEPTION 'DEMO_PHYSICAL_LEASE_SCOPE_INVALID'; END IF;
 -- Session lock survives transaction boundaries and excludes another alias
 -- even if the logical TTL expires while this backend is still alive.
 held:=pg_try_advisory_lock(52211463,0);
 IF NOT held THEN RAISE EXCEPTION 'DEMO_PHYSICAL_LEASE_HELD'; END IF;
 SELECT * INTO prior FROM target.demo_physical_lease WHERE broker_account_id=52211463 FOR UPDATE;
 IF FOUND AND prior.backend_pid=pg_backend_pid()
 THEN RAISE EXCEPTION 'DEMO_PHYSICAL_LEASE_HELD'; END IF;
 gen:=target.acquire_executor_lease(p_account,p_worker,p_ttl);
 INSERT INTO target.demo_physical_lease VALUES(52211463,p_account,p_worker,pg_backend_pid(),gen,clock_timestamp()+make_interval(secs=>p_ttl))
 ON CONFLICT(broker_account_id) DO UPDATE SET account_id=excluded.account_id,worker_id=excluded.worker_id,
 backend_pid=excluded.backend_pid,generation=excluded.generation,expires_at=excluded.expires_at;
 RETURN gen;
EXCEPTION WHEN OTHERS THEN
 IF held THEN PERFORM pg_advisory_unlock(52211463,0); END IF;
 RAISE;
END $$;
CREATE FUNCTION target.assert_demo_executor_lease(p_account text,p_worker text,p_generation bigint)
RETURNS void LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog,pg_temp AS $$
BEGIN
 PERFORM target.assert_executor_lease(p_account,p_worker,p_generation);
 PERFORM 1 FROM target.demo_physical_lease WHERE broker_account_id=52211463 AND account_id=p_account
 AND worker_id=p_worker AND generation=p_generation AND backend_pid=pg_backend_pid()
 AND expires_at>clock_timestamp() FOR UPDATE;
 IF NOT FOUND OR NOT EXISTS(SELECT 1 FROM pg_locks WHERE locktype='advisory' AND pid=pg_backend_pid()
 AND classid=52211463::oid AND objid=0::oid AND objsubid=2 AND granted)
 THEN RAISE EXCEPTION 'DEMO_PHYSICAL_FENCE_INVALID'; END IF;
END $$;
CREATE OR REPLACE FUNCTION target.renew_executor_lease(p_account text,p_worker text,p_generation bigint,p_ttl integer)
RETURNS void LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog,pg_temp AS $$
BEGIN
 IF p_ttl IS NULL OR p_ttl NOT BETWEEN 1 AND 3600 THEN RAISE EXCEPTION 'INVALID_TARGET_TTL'; END IF;
 PERFORM target.assert_executor_lease(p_account,p_worker,p_generation);
 UPDATE target.executor_leases SET expires_at=clock_timestamp()+make_interval(secs=>p_ttl) WHERE account_id=p_account;
 UPDATE target.demo_physical_lease SET expires_at=clock_timestamp()+make_interval(secs=>p_ttl)
 WHERE broker_account_id=52211463 AND account_id=p_account AND worker_id=p_worker
 AND generation=p_generation AND backend_pid=pg_backend_pid();
END $$;
CREATE OR REPLACE FUNCTION target.release_executor_lease(p_account text,p_worker text,p_generation bigint)
RETURNS void LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog,pg_temp AS $$
BEGIN
 PERFORM target.assert_executor_lease(p_account,p_worker,p_generation);
 UPDATE target.executor_leases SET expires_at='epoch' WHERE account_id=p_account;
 UPDATE target.demo_physical_lease SET expires_at='epoch',backend_pid=0
 WHERE broker_account_id=52211463 AND account_id=p_account AND worker_id=p_worker
 AND generation=p_generation AND backend_pid=pg_backend_pid();
 IF FOUND THEN PERFORM pg_advisory_unlock(52211463,0); END IF;
END $$;
REVOKE ALL ON FUNCTION target.acquire_demo_executor_lease(text,text,integer),target.assert_demo_executor_lease(text,text,bigint) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION target.acquire_demo_executor_lease(text,text,integer),target.assert_demo_executor_lease(text,text,bigint) TO trady_executor;

-- Only a root/migrator import of an independently reviewed, exact-head
-- native cleanup may exclude a stale journal from physical admission. Runtime
-- roles have no table-write grant. The journal itself is never rewritten.
CREATE TABLE target.reviewed_external_cleanups (
 owner_id text NOT NULL,account_id text NOT NULL,intent_id text NOT NULL,
 head_revision bigint NOT NULL,state_sha256 text NOT NULL CHECK(state_sha256 ~ '^[a-f0-9]{64}$'),
 evidence_sha256 text NOT NULL CHECK(evidence_sha256 ~ '^[a-f0-9]{64}$'),
 review_sha256 text NOT NULL CHECK(review_sha256 ~ '^[a-f0-9]{64}$'),
 reviewed_at timestamptz NOT NULL, valid_until timestamptz NOT NULL,
 PRIMARY KEY(owner_id,account_id,intent_id,head_revision,state_sha256,review_sha256)
);
REVOKE ALL ON target.reviewed_external_cleanups FROM PUBLIC;
CREATE FUNCTION target.protocol_is_externally_cleaned(p_owner text,p_account text,p_intent text) RETURNS boolean
LANGUAGE sql SECURITY DEFINER SET search_path=pg_catalog,pg_temp AS $$
 SELECT EXISTS(SELECT 1 FROM target.reviewed_external_cleanups c JOIN target.protocol_heads h USING(owner_id,account_id,intent_id)
 WHERE h.owner_id=p_owner AND h.account_id=p_account AND h.intent_id=p_intent
 AND c.head_revision=h.revision AND c.state_sha256=h.state_sha256 AND c.reviewed_at<=clock_timestamp());
$$;
REVOKE ALL ON FUNCTION target.protocol_is_externally_cleaned(text,text,text) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION target.protocol_is_externally_cleaned(text,text,text) TO trady_executor;
CREATE FUNCTION target.physical_unresolved_protocols() RETURNS jsonb
LANGUAGE sql SECURITY DEFINER SET search_path=pg_catalog,pg_temp AS $$
 SELECT COALESCE(jsonb_agg(jsonb_build_object('owner_id',h.owner_id,'account_id',h.account_id,'intent_id',h.intent_id,
 'revision',h.revision,'state_sha256',h.state_sha256,'phase',e.document->'state'->>'phase') ORDER BY h.owner_id,h.account_id,h.intent_id),'[]'::jsonb)
 FROM target.protocol_heads h JOIN target.protocol_events e USING(owner_id,account_id,intent_id,revision)
 WHERE h.context->>'runtime_mode'='DEMO' AND h.context->>'broker_account_id'='52211463'
 AND e.document->'state'->>'phase' NOT IN ('FLAT','REJECTED')
 AND NOT EXISTS(SELECT 1 FROM target.reviewed_external_cleanups c WHERE c.owner_id=h.owner_id AND c.account_id=h.account_id AND c.intent_id=h.intent_id
 AND c.head_revision=h.revision AND c.state_sha256=h.state_sha256 AND c.reviewed_at<=clock_timestamp() AND c.valid_until>clock_timestamp());
$$;
REVOKE ALL ON FUNCTION target.physical_unresolved_protocols() FROM PUBLIC;
GRANT EXECUTE ON FUNCTION target.physical_unresolved_protocols() TO trady_executor,trady_analyst;
-- Historical reports preserve permanent exact-head closure after entry-review
-- expiry. This function is never used by admission, SHADOW or the order driver.
CREATE FUNCTION target.physical_unresolved_protocols_for_audit() RETURNS jsonb
LANGUAGE sql SECURITY DEFINER SET search_path=pg_catalog,pg_temp AS $$
 SELECT COALESCE(jsonb_agg(jsonb_build_object('owner_id',h.owner_id,'account_id',h.account_id,'intent_id',h.intent_id,
 'revision',h.revision,'state_sha256',h.state_sha256,'phase',e.document->'state'->>'phase') ORDER BY h.owner_id,h.account_id,h.intent_id),'[]'::jsonb)
 FROM target.protocol_heads h JOIN target.protocol_events e USING(owner_id,account_id,intent_id,revision)
 WHERE h.context->>'runtime_mode'='DEMO' AND h.context->>'broker_account_id'='52211463'
 AND e.document->'state'->>'phase' NOT IN ('FLAT','REJECTED')
 AND NOT target.protocol_is_externally_cleaned(h.owner_id,h.account_id,h.intent_id);
$$;
REVOKE ALL ON FUNCTION target.physical_unresolved_protocols_for_audit() FROM PUBLIC;
GRANT EXECUTE ON FUNCTION target.physical_unresolved_protocols_for_audit() TO trady_executor;

CREATE FUNCTION target.enforce_physical_entry_budget() RETURNS trigger
LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog,pg_temp AS $$
DECLARE lease target.demo_physical_lease%ROWTYPE; day date; cycle text;
BEGIN
 IF NEW.context->>'runtime_mode'='REPLAY' THEN RETURN NEW; END IF;
 IF NEW.context->>'runtime_mode' IS DISTINCT FROM 'DEMO' THEN RAISE EXCEPTION 'PROTOCOL_RUNTIME_MODE_INVALID'; END IF;
 SELECT * INTO lease FROM target.demo_physical_lease WHERE broker_account_id=52211463;
 IF NOT FOUND OR lease.account_id<>NEW.account_id THEN RAISE EXCEPTION 'DEMO_PHYSICAL_FENCE_INVALID'; END IF;
 PERFORM target.assert_demo_executor_lease(NEW.account_id,lease.worker_id,lease.generation);
 -- Every update remains physically fenced, but an original intent may recover
 -- after its day/cycle budget was spent or its entry authority expired.
 IF EXISTS(SELECT 1 FROM target.protocol_heads WHERE owner_id=NEW.owner_id AND account_id=NEW.account_id AND intent_id=NEW.intent_id) THEN
   IF EXISTS(SELECT 1 FROM target.reviewed_external_cleanups c JOIN target.protocol_heads h USING(owner_id,account_id,intent_id)
     WHERE h.owner_id=NEW.owner_id AND h.account_id=NEW.account_id AND h.intent_id=NEW.intent_id
     AND c.head_revision=h.revision AND c.state_sha256=h.state_sha256 AND c.reviewed_at<=clock_timestamp())
   THEN RAISE EXCEPTION 'DEMO_EXTERNALLY_CLEANED_INTENT_READ_ONLY'; END IF;
   RETURN NEW;
 END IF;
 -- A durable retirement with no entry/STOP/exit cannot place an order and
 -- consumes no stock-entry slot. Old journals/reservations stay unchanged.
 IF NEW.revision=1 AND EXISTS(SELECT 1 FROM target.protocol_events e WHERE e.owner_id=NEW.owner_id AND e.account_id=NEW.account_id
 AND e.intent_id=NEW.intent_id AND e.revision=1 AND e.document->'input'->>'kind'='REQUEST_EXIT'
 AND e.document->'input'->>'reason'='UNSUBMITTED_PREPARATION_RETIRED'
 AND e.document->'state'->>'phase'='FLAT' AND e.document->'state'->'entry'='null'::jsonb
 AND e.document->'state'->'protection'='null'::jsonb AND e.document->'state'->'exits'='[]'::jsonb)
 THEN RETURN NEW; END IF;
 IF jsonb_array_length(target.physical_unresolved_protocols())>0 THEN RAISE EXCEPTION 'DEMO_PHYSICAL_UNRESOLVED_INTENT'; END IF;
 day:=((NEW.context->>'created_at')::timestamptz AT TIME ZONE 'UTC')::date;
 IF day<>(clock_timestamp() AT TIME ZONE 'UTC')::date THEN RAISE EXCEPTION 'DEMO_NEW_ENTRY_DAY_INVALID'; END IF;
 cycle:=NEW.context->>'research_cycle_id';
 IF cycle IS NOT NULL AND cycle !~ '^[A-Za-z0-9_.:-]{1,120}$' THEN RAISE EXCEPTION 'DEMO_RESEARCH_CYCLE_INVALID'; END IF;
 PERFORM pg_advisory_xact_lock(52211463,(day-DATE '2000-01-01')::integer+1);
 IF (SELECT count(*) FROM target.physical_entry_reservations WHERE environment='DEMO' AND broker_account_id=52211463 AND session_day=day)>=30
 THEN RAISE EXCEPTION 'DEMO_PHYSICAL_DAILY_ENTRY_LIMIT_30'; END IF;
 INSERT INTO target.physical_entry_reservations VALUES('DEMO',52211463,day,NEW.owner_id,NEW.account_id,NEW.intent_id,cycle,clock_timestamp());
 RETURN NEW;
END $$;
REVOKE ALL ON FUNCTION target.enforce_physical_entry_budget() FROM PUBLIC;
CREATE TRIGGER physical_entry_budget BEFORE INSERT OR UPDATE ON target.protocol_heads
FOR EACH ROW EXECUTE FUNCTION target.enforce_physical_entry_budget();
ALTER TABLE target.daily_entry_slots DROP CONSTRAINT daily_entry_slots_pkey;
ALTER TABLE target.daily_entry_slots ADD PRIMARY KEY(owner_id,account_id,session_day,intent_id);

CREATE OR REPLACE FUNCTION target.append_protocol_event(p_owner text,p_account text,p_intent text,
 p_expected bigint,p_document jsonb,p_canonical text,p_hash text,p_state_canonical text,
 p_worker text,p_generation bigint)
RETURNS boolean LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog,pg_temp AS $$
DECLARE head target.protocol_heads%ROWTYPE; old target.protocol_events%ROWTYPE;
 state jsonb; ctx jsonb; rev bigint; slot text; event text;
BEGIN
 IF session_user<>'trady_executor' THEN RAISE EXCEPTION 'PROTOCOL_WRITER_FORBIDDEN'; END IF;
 PERFORM target.assert_executor_lease(p_account,p_worker,p_generation);
 IF p_owner IS NULL OR p_owner !~ '^[A-Za-z0-9_.:-]{1,120}$'
 OR p_intent IS NULL OR p_intent !~ '^[A-Za-z0-9_.:-]{1,120}$'
 OR p_expected IS NULL OR p_expected NOT BETWEEN 0 AND 9223372036854775806
 OR p_document IS NULL OR p_canonical IS NULL OR p_state_canonical IS NULL
 OR octet_length(p_canonical)>1000000 OR octet_length(p_state_canonical)>500000
 OR p_canonical::jsonb IS DISTINCT FROM p_document
 OR p_hash IS DISTINCT FROM encode(sha256(convert_to(p_canonical,'UTF8')),'hex')
 THEN RAISE EXCEPTION 'INVALID_PROTOCOL_RECORD'; END IF;
 state:=p_document->'state';ctx:=state->'context';event:=p_document->>'event_id';
 IF state IS DISTINCT FROM p_state_canonical::jsonb OR ctx->>'intent_id' IS DISTINCT FROM p_intent
 OR ctx->>'account_id' IS DISTINCT FROM p_account OR ctx->>'broker_account_id' IS DISTINCT FROM '52211463'
 OR event IS NULL OR event !~ '^[A-Za-z0-9_.:-]{1,120}$'
 OR p_document->>'previous_hash' IS NULL OR p_document->>'previous_hash' !~ '^[a-f0-9]{64}$'
 THEN RAISE EXCEPTION 'INVALID_PROTOCOL_SCOPE'; END IF;
 rev:=(state->>'revision')::bigint;
 IF rev IS DISTINCT FROM p_expected+1 THEN RAISE EXCEPTION 'PROTOCOL_REVISION_CONFLICT'; END IF;
 SELECT * INTO old FROM target.protocol_events WHERE owner_id=p_owner AND account_id=p_account
 AND intent_id=p_intent AND event_id=event;
 IF FOUND THEN
  IF old.document IS DISTINCT FROM p_document OR old.content_sha256<>p_hash THEN RAISE EXCEPTION 'PROTOCOL_EVENT_REBOUND'; END IF;
  RETURN false;
 END IF;
 SELECT * INTO head FROM target.protocol_heads WHERE owner_id=p_owner AND account_id=p_account AND intent_id=p_intent FOR UPDATE;
 IF FOUND THEN
  IF head.revision<>p_expected OR head.state_sha256 IS DISTINCT FROM p_document->>'previous_hash'
  OR head.context IS DISTINCT FROM ctx THEN RAISE EXCEPTION 'PROTOCOL_HEAD_CONFLICT'; END IF;
 ELSE
  IF p_expected<>0 OR state->>'holding_quantity' IS DISTINCT FROM '0'
  OR state->'entry' IS DISTINCT FROM 'null'::jsonb OR state->'protection' IS DISTINCT FROM 'null'::jsonb
  OR state->'exits' IS DISTINCT FROM '[]'::jsonb THEN RAISE EXCEPTION 'PROTOCOL_RESERVATION_REQUIRED'; END IF;
  IF p_document->'input'->>'kind'='RESERVE' AND state->>'phase'='PREPARED' THEN
  INSERT INTO target.daily_entry_slots(owner_id,account_id,session_day,intent_id)
  VALUES(p_owner,p_account,((ctx->>'created_at')::timestamptz AT TIME ZONE 'UTC')::date,p_intent) ON CONFLICT DO NOTHING;
  IF ctx->>'runtime_mode'<>'DEMO' THEN
    SELECT intent_id INTO STRICT slot FROM target.daily_entry_slots WHERE owner_id=p_owner AND account_id=p_account
     AND session_day=((ctx->>'created_at')::timestamptz AT TIME ZONE 'UTC')::date;
    IF slot<>p_intent THEN RAISE EXCEPTION 'EUR_DAILY_ENTRY_SLOT_SPENT'; END IF;
  END IF;
  ELSIF p_document->'input'->>'kind'='REQUEST_EXIT' AND p_document->'input'->>'reason'='UNSUBMITTED_PREPARATION_RETIRED'
    AND state->>'phase'='FLAT' AND state->>'exit_requested'='true' THEN
    NULL;
  ELSE RAISE EXCEPTION 'PROTOCOL_RESERVATION_REQUIRED'; END IF;
 END IF;
 INSERT INTO target.protocol_events VALUES(p_owner,p_account,p_intent,rev,event,p_document,p_hash,clock_timestamp());
 INSERT INTO target.protocol_heads VALUES(p_owner,p_account,p_intent,rev,encode(sha256(convert_to(p_state_canonical,'UTF8')),'hex'),ctx)
 ON CONFLICT(owner_id,account_id,intent_id) DO UPDATE SET revision=excluded.revision,state_sha256=excluded.state_sha256;
 RETURN true;
END $$;

CREATE OR REPLACE FUNCTION target.daily_entry_count(p_owner text,p_account text,p_day date)
RETURNS bigint LANGUAGE sql SECURITY DEFINER SET search_path=pg_catalog,pg_temp AS $$
 SELECT CASE WHEN p_account LIKE 'paper\_%' ESCAPE '\' THEN
 (SELECT count(*) FROM target.physical_entry_reservations WHERE environment='DEMO' AND broker_account_id=52211463 AND session_day=p_day)
 ELSE (SELECT count(*) FROM target.daily_entry_slots WHERE owner_id=p_owner AND account_id=p_account AND session_day=p_day) END;
$$;
