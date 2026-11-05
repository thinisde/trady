CREATE INDEX eur_record_window_index ON target.records(kind,owner_id,account_id,created_at,record_id);
-- Bounded read-only keyset pages for cumulative EUR evidence collection.
CREATE FUNCTION target.list_record_window(p_kind text,p_owner text,p_account text,
 p_start timestamptz,p_end timestamptz,p_after timestamptz,p_after_id text,p_limit integer)
RETURNS jsonb LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog,pg_temp AS $$
BEGIN
 IF p_start IS NULL OR p_end IS NULL OR p_start>=p_end OR p_end-p_start>interval '31 days'
 OR p_limit IS NULL OR p_limit NOT BETWEEN 1 AND 64
 OR (p_after IS NULL) IS DISTINCT FROM (p_after_id IS NULL)
 THEN RAISE EXCEPTION 'INVALID_EUR_RECORD_WINDOW'; END IF;
 RETURN (SELECT COALESCE(jsonb_agg(jsonb_build_object('record_id',record_id,'document',document,
  'content_sha256',content_sha256,'created_at',created_at) ORDER BY created_at,record_id),'[]'::jsonb)
 FROM (SELECT * FROM target.records WHERE kind=p_kind AND owner_id=p_owner AND account_id=p_account
  AND created_at>=p_start AND created_at<p_end
  AND (p_after IS NULL OR (created_at,record_id)>(p_after,p_after_id))
  ORDER BY created_at,record_id LIMIT p_limit) selected_rows);
END $$;
REVOKE ALL ON FUNCTION target.list_record_window(text,text,text,timestamptz,timestamptz,timestamptz,text,integer) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION target.list_record_window(text,text,text,timestamptz,timestamptz,timestamptz,text,integer) TO trady_executor,trady_analyst;

