-- A dead PostgreSQL backend cannot continue an executor lease. Permit prompt
-- fenced restart after a crash while retaining exclusion for a live worker.
CREATE OR REPLACE FUNCTION target.acquire_executor_lease(p_account text,p_worker text,p_ttl integer)
RETURNS bigint LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog,pg_temp AS $$
DECLARE old target.executor_leases%ROWTYPE;
BEGIN
 IF p_account IS NULL OR p_account !~ '^(paper|shadow|replay)_[A-Za-z0-9_-]{1,89}$'
 OR p_worker IS NULL OR p_worker !~ '^[A-Za-z0-9_-]{1,96}$'
 OR p_ttl IS NULL OR p_ttl NOT BETWEEN 1 AND 3600 THEN RAISE EXCEPTION 'INVALID_TARGET_LEASE'; END IF;
 INSERT INTO target.executor_leases VALUES(p_account,p_worker,pg_backend_pid(),1,'epoch') ON CONFLICT DO NOTHING;
 SELECT * INTO STRICT old FROM target.executor_leases WHERE account_id=p_account FOR UPDATE;
 IF old.expires_at>clock_timestamp() AND EXISTS (
   SELECT 1 FROM pg_stat_activity WHERE pid=old.backend_pid AND datname=current_database())
 THEN RAISE EXCEPTION 'TARGET_LEASE_HELD'; END IF;
 UPDATE target.executor_leases SET worker_id=p_worker,backend_pid=pg_backend_pid(),
 generation=old.generation+1,expires_at=clock_timestamp()+make_interval(secs=>p_ttl) WHERE account_id=p_account;
 RETURN old.generation+1;
END $$;

CREATE FUNCTION target.telegram_delivery_summary(p_deployment text,p_owner text,p_account text)
RETURNS jsonb LANGUAGE sql SECURITY DEFINER SET search_path=pg_catalog,pg_temp AS $$
 SELECT COALESCE(jsonb_agg(jsonb_build_object('event_id',event_id,'notification_kind',notification_kind,
  'status',status,'attempt_count',attempt_count,'sent_at',sent_at,'last_error',last_error)
  ORDER BY event_id,notification_kind),'[]'::jsonb)
 FROM target.telegram_outbox WHERE deployment_id=p_deployment AND owner_id=p_owner AND account_id=p_account;
$$;
REVOKE ALL ON FUNCTION target.telegram_delivery_summary(text,text,text) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION target.telegram_delivery_summary(text,text,text) TO trady_analyst,trady_executor;
