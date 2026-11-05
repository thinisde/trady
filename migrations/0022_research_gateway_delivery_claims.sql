-- Claim before transport. Telegram has no client idempotency key: SENDING and
-- UNKNOWN are never automatically reclaimed after a crash or uncertain send.
ALTER TABLE control.gateway_outbox
 ADD COLUMN delivery_status text NOT NULL DEFAULT 'PENDING'
 CHECK(delivery_status IN ('PENDING','SENDING','SENT','UNKNOWN','SUPPRESSED')),
 ADD COLUMN delivery_token text,
 ADD COLUMN claimed_at timestamptz;
-- Old undelivered rows have no dispatch marker, so an earlier send cannot be
-- ruled out. Preserve them for review instead of inventing an unsent outcome.
UPDATE control.gateway_outbox SET delivery_status=CASE WHEN delivered_at IS NOT NULL THEN 'SENT' ELSE 'UNKNOWN' END;
CREATE INDEX gateway_outbox_pending ON control.gateway_outbox(bot_id,created_at,delivery_id)
 WHERE delivery_status='PENDING';
REVOKE UPDATE(delivered_at) ON control.gateway_outbox FROM trady_gateway;

CREATE FUNCTION control.claim_gateway_delivery(p_bot bigint,p_token text)
RETURNS jsonb LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog,pg_temp AS $$
DECLARE item control.gateway_outbox%ROWTYPE;
BEGIN
 IF session_user<>'trady_gateway' OR p_bot IS NULL OR p_bot<=0
 OR p_token IS NULL OR p_token !~ '^delivery_claim_[a-f0-9]{32}$'
 THEN RAISE EXCEPTION 'INVALID_GATEWAY_DELIVERY_CLAIM'; END IF;
 SELECT * INTO item FROM control.gateway_outbox WHERE bot_id=p_bot AND delivery_status='PENDING'
 ORDER BY created_at,delivery_id FOR UPDATE SKIP LOCKED LIMIT 1;
 IF NOT FOUND THEN RETURN NULL; END IF;
 UPDATE control.gateway_outbox SET delivery_status='SENDING',delivery_token=p_token,claimed_at=clock_timestamp()
 WHERE delivery_id=item.delivery_id;
 RETURN jsonb_build_object('id',item.delivery_id,'bot_id',item.bot_id,'owner_user_id',item.owner_user_id,
  'chat_id',item.chat_id,'scope',jsonb_build_object('owner_id',item.owner_id,'account_id',item.account_id,
  'policy_version',item.policy_version),'text',item.text);
END $$;

CREATE FUNCTION control.finish_gateway_delivery(p_bot bigint,p_id text,p_token text,p_outcome text)
RETURNS void LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog,pg_temp AS $$
BEGIN
 IF session_user<>'trady_gateway' OR p_outcome IS NULL OR p_outcome NOT IN ('SENT','UNKNOWN','SUPPRESSED')
 OR p_bot IS NULL OR p_id IS NULL OR p_token IS NULL
 THEN RAISE EXCEPTION 'INVALID_GATEWAY_DELIVERY_RESULT'; END IF;
 UPDATE control.gateway_outbox SET delivery_status=p_outcome,
  delivered_at=CASE WHEN p_outcome='SENT' THEN clock_timestamp() ELSE NULL END
 WHERE delivery_id=p_id AND bot_id=p_bot AND delivery_token=p_token AND delivery_status='SENDING';
 IF NOT FOUND THEN RAISE EXCEPTION 'GATEWAY_DELIVERY_FENCE_INVALID'; END IF;
END $$;
REVOKE ALL ON FUNCTION control.claim_gateway_delivery(bigint,text),
 control.finish_gateway_delivery(bigint,text,text,text) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION control.claim_gateway_delivery(bigint,text),
 control.finish_gateway_delivery(bigint,text,text,text) TO trady_gateway;
