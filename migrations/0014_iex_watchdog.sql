-- Persist the fenced IEX quote-stream watchdog while a holding is active.
-- The executor can retry a bucket without duplicating owner notifications.

CREATE FUNCTION execution.record_watchdog_observation(
    p_account text,
    p_worker text,
    p_generation bigint,
    p_document jsonb
) RETURNS void
LANGUAGE plpgsql SECURITY DEFINER SET search_path = pg_catalog, pg_temp AS $$
DECLARE event_key text;
BEGIN
    PERFORM execution.assert_trader(p_account,p_worker,p_generation);
    IF jsonb_typeof(p_document) IS DISTINCT FROM 'object'
       OR NOT (p_document ?& ARRAY[
           'bucket','observed_at','status','holding','stream_connected',
           'stream_reconnects','last_error','stale_symbols'
       ])
       OR coalesce(p_document->>'bucket','') !~ '^[0-9]+$'
       OR (p_document->>'status') NOT IN ('HEALTHY','DEGRADED','STOPPED')
       OR jsonb_typeof(p_document->'holding') IS DISTINCT FROM 'boolean'
       OR jsonb_typeof(p_document->'stream_connected') IS DISTINCT FROM 'boolean'
       OR coalesce(p_document->>'stream_reconnects','') !~ '^[0-9]+$'
       OR (p_document->>'last_error' IS NOT NULL AND length(p_document->>'last_error') > 256)
       OR jsonb_typeof(p_document->'stale_symbols') IS DISTINCT FROM 'array'
       OR jsonb_array_length(p_document->'stale_symbols') > 30
       OR (p_document->>'observed_at')::timestamptz > clock_timestamp()+interval '5 seconds'
       THEN RAISE EXCEPTION 'invalid IEX watchdog observation'; END IF;
    event_key='iex-watchdog:'||p_account||':'||(p_document->>'bucket');
    INSERT INTO audit.events(actor,event_type,account_id,payload)
        VALUES(session_user,'IEX_WATCHDOG',p_account,jsonb_build_object(
            'generation',p_generation,'observation',p_document
        ));
    INSERT INTO control.notification_outbox(event_key,payload,producer)
        VALUES(event_key,jsonb_build_object(
            'event','IEX_WATCHDOG','account_id',p_account,'observation',p_document
        ),session_user)
        ON CONFLICT DO NOTHING;
END;
$$;

REVOKE ALL ON FUNCTION execution.record_watchdog_observation(text,text,bigint,jsonb) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION execution.record_watchdog_observation(text,text,bigint,jsonb) TO trady_executor;
