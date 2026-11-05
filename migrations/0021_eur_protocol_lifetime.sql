-- Active protocols must not lose recovery or exit authority at an arbitrary
-- event count. Preserve the positive bigint revision, fencing and hash chain.
ALTER TABLE target.protocol_heads DROP CONSTRAINT protocol_heads_revision_check;
ALTER TABLE target.protocol_heads ADD CONSTRAINT protocol_heads_revision_check CHECK(revision > 0);

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
  IF p_expected<>0 OR p_document->'input'->>'kind' IS DISTINCT FROM 'RESERVE'
  OR state->>'phase' IS DISTINCT FROM 'PREPARED' OR state->>'holding_quantity' IS DISTINCT FROM '0'
  OR state->'entry' IS DISTINCT FROM 'null'::jsonb OR state->'protection' IS DISTINCT FROM 'null'::jsonb
  OR state->'exits' IS DISTINCT FROM '[]'::jsonb THEN RAISE EXCEPTION 'PROTOCOL_RESERVATION_REQUIRED'; END IF;
  INSERT INTO target.daily_entry_slots(owner_id,account_id,session_day,intent_id)
  VALUES(p_owner,p_account,((ctx->>'created_at')::timestamptz AT TIME ZONE 'UTC')::date,p_intent) ON CONFLICT DO NOTHING;
  SELECT intent_id INTO STRICT slot FROM target.daily_entry_slots WHERE owner_id=p_owner AND account_id=p_account
   AND session_day=((ctx->>'created_at')::timestamptz AT TIME ZONE 'UTC')::date;
  IF slot<>p_intent THEN RAISE EXCEPTION 'EUR_DAILY_ENTRY_SLOT_SPENT'; END IF;
 END IF;
 INSERT INTO target.protocol_events VALUES(p_owner,p_account,p_intent,rev,event,p_document,p_hash,clock_timestamp());
 INSERT INTO target.protocol_heads VALUES(p_owner,p_account,p_intent,rev,encode(sha256(convert_to(p_state_canonical,'UTF8')),'hex'),ctx)
 ON CONFLICT(owner_id,account_id,intent_id) DO UPDATE SET revision=excluded.revision,state_sha256=excluded.state_sha256;
 RETURN true;
END $$;
