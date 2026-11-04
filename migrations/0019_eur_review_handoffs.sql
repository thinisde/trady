-- Review artifacts contain identifiers, hashes and metrics, never execution authority.
CREATE TABLE target.review_artifacts (
 owner_id text NOT NULL, account_id text NOT NULL, artifact_id text NOT NULL,
 document jsonb NOT NULL, content_sha256 text NOT NULL,
 created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 PRIMARY KEY(owner_id,account_id,artifact_id),
 CHECK(document->>'status'='PENDING_REVIEW')
);
REVOKE ALL ON target.review_artifacts FROM PUBLIC;
CREATE FUNCTION target.list_records(p_kind text,p_owner text,p_account text,p_since timestamptz,p_limit integer)
RETURNS jsonb LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog,pg_temp AS $$
BEGIN
 IF p_limit IS NULL OR p_limit NOT BETWEEN 1 AND 1000 OR p_since IS NULL THEN RAISE EXCEPTION 'INVALID_RECORD_LIST'; END IF;
 RETURN (SELECT COALESCE(jsonb_agg(jsonb_build_object('record_id',record_id,'document',document,
  'content_sha256',content_sha256,'created_at',created_at) ORDER BY created_at,record_id),'[]'::jsonb)
 FROM (SELECT * FROM target.records WHERE owner_id=p_owner AND account_id=p_account
  AND kind=p_kind AND created_at>=p_since ORDER BY created_at,record_id LIMIT p_limit) r);
END $$;
CREATE FUNCTION target.append_review_artifact(p_owner text,p_account text,p_id text,p_document jsonb,p_canonical text,p_hash text)
RETURNS boolean LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog,pg_temp AS $$
DECLARE old target.review_artifacts%ROWTYPE;
BEGIN
 IF session_user<>'trady_analyst' OR p_id IS NULL OR p_id !~ '^[A-Za-z0-9_.:-]{1,120}$'
 OR p_owner IS NULL OR p_owner !~ '^[A-Za-z0-9_.:-]{1,120}$'
 OR p_account IS NULL OR p_account !~ '^(paper|replay)_[A-Za-z0-9_-]{1,89}$'
 OR p_document IS NULL OR jsonb_typeof(p_document) IS DISTINCT FROM 'object'
 OR p_document->>'schema_version' IS DISTINCT FROM '1'
 OR p_document->>'status' IS DISTINCT FROM 'PENDING_REVIEW'
 OR p_document->>'bundle_id' IS DISTINCT FROM p_id OR p_canonical IS NULL
 OR octet_length(p_canonical)>1000000 OR p_canonical::jsonb IS DISTINCT FROM p_document
 OR p_hash IS DISTINCT FROM encode(sha256(convert_to(p_canonical,'UTF8')),'hex')
 OR (p_document-ARRAY['schema_version','bundle_id','status','report_ids','evidence_ids','hashes','metrics',
  'review_questions','policy_revision','configuration_hash','period_start','period_end','generated_at','code_hash'])<>'{}'::jsonb
 OR jsonb_typeof(p_document->'report_ids') IS DISTINCT FROM 'array'
 OR jsonb_typeof(p_document->'evidence_ids') IS DISTINCT FROM 'array'
 OR jsonb_typeof(p_document->'metrics') IS DISTINCT FROM 'object'
 OR jsonb_typeof(p_document->'hashes') IS DISTINCT FROM 'object'
 OR jsonb_typeof(p_document->'review_questions') IS DISTINCT FROM 'array'
 OR COALESCE(p_document->>'policy_revision','') !~ '^[A-Za-z0-9_.:-]{1,160}$'
 OR COALESCE(p_document->>'configuration_hash','') !~ '^[a-f0-9]{64}$'
 OR COALESCE(p_document->>'code_hash','') !~ '^[a-f0-9]{64}$'
 OR COALESCE(p_document->>'period_start','') !~ '^[0-9T:.+Z-]{20,40}$'
 OR COALESCE(p_document->>'period_end','') !~ '^[0-9T:.+Z-]{20,40}$'
 OR COALESCE(p_document->>'generated_at','') !~ '^[0-9T:.+Z-]{20,40}$'
 THEN RAISE EXCEPTION 'INVALID_SANITIZED_REVIEW_ARTIFACT'; END IF;
 IF jsonb_array_length(p_document->'report_ids') NOT BETWEEN 1 AND 256 OR jsonb_array_length(p_document->'evidence_ids')>256
 OR jsonb_array_length(p_document->'review_questions')>20
 OR EXISTS(SELECT 1 FROM jsonb_array_elements(p_document->'review_questions')
  WHERE jsonb_typeof(value)<>'string' OR octet_length(value::text)>1024)
 OR EXISTS(SELECT 1 FROM jsonb_each(p_document->'metrics') WHERE jsonb_typeof(value) NOT IN ('number','boolean'))
 OR EXISTS(SELECT 1 FROM jsonb_each_text(p_document->'hashes') WHERE value !~ '^[a-f0-9]{64}$')
 OR EXISTS(SELECT 1 FROM jsonb_array_elements_text((p_document->'report_ids')||(p_document->'evidence_ids'))
  WHERE value !~ '^[A-Za-z0-9_.:-]{1,160}$')
 THEN RAISE EXCEPTION 'REVIEW_CONTENT_NOT_SANITIZED'; END IF;
 INSERT INTO target.review_artifacts(owner_id,account_id,artifact_id,document,content_sha256)
 VALUES(p_owner,p_account,p_id,p_document,p_hash) ON CONFLICT DO NOTHING;
 IF FOUND THEN RETURN true; END IF;
 SELECT * INTO old FROM target.review_artifacts WHERE owner_id=p_owner AND account_id=p_account AND artifact_id=p_id;
 IF old.document IS DISTINCT FROM p_document OR old.content_sha256<>p_hash THEN RAISE EXCEPTION 'REVIEW_ARTIFACT_REBOUND'; END IF;
 RETURN false;
END $$;
CREATE FUNCTION target.read_review_artifact(p_owner text,p_account text,p_id text)
RETURNS jsonb LANGUAGE sql SECURITY DEFINER SET search_path=pg_catalog,pg_temp AS $$
 SELECT document FROM target.review_artifacts WHERE owner_id=p_owner AND account_id=p_account AND artifact_id=p_id;
$$;
REVOKE ALL ON FUNCTION target.list_records(text,text,text,timestamptz,integer),
 target.append_review_artifact(text,text,text,jsonb,text,text),target.read_review_artifact(text,text,text) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION target.list_records(text,text,text,timestamptz,integer),target.read_review_artifact(text,text,text) TO trady_analyst,trady_executor;
GRANT EXECUTE ON FUNCTION target.append_review_artifact(text,text,text,jsonb,text,text) TO trady_analyst;
