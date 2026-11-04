-- EUR target authority records. Runtime services write append-only JSONB
-- documents through narrowly scoped role functions; no table is an order
-- authority by itself.
CREATE SCHEMA IF NOT EXISTS target;
REVOKE ALL ON SCHEMA target FROM PUBLIC;

CREATE TABLE IF NOT EXISTS target.records (
    kind text NOT NULL,
    record_id text NOT NULL,
    owner_id text NOT NULL,
    account_id text NOT NULL,
    document jsonb NOT NULL,
    content_sha256 text NOT NULL,
    created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    PRIMARY KEY(kind,record_id),
    CHECK (kind ~ '^[a-z][a-z0-9_]{1,63}$'),
    CHECK (record_id ~ '^[A-Za-z0-9][A-Za-z0-9_.:-]{0,159}$'),
    CHECK (owner_id ~ '^[A-Za-z0-9][A-Za-z0-9_.:-]{0,159}$'),
    CHECK (account_id ~ '^(paper|shadow|live)_[A-Za-z0-9_.:-]{1,127}$'),
    CHECK (content_sha256 ~ '^[0-9a-f]{64}$')
);
REVOKE ALL ON target.records FROM PUBLIC;

CREATE TABLE IF NOT EXISTS target.eur_universe_snapshots (
    snapshot_id text PRIMARY KEY,
    owner_id text NOT NULL,
    document jsonb NOT NULL,
    content_sha256 text NOT NULL CHECK (content_sha256 ~ '^[0-9a-f]{64}$'),
    effective_from timestamptz NOT NULL,
    expires_at timestamptz NOT NULL,
    created_at timestamptz NOT NULL DEFAULT clock_timestamp()
);
CREATE TABLE IF NOT EXISTS target.quote_snapshots (
    quote_id text PRIMARY KEY,
    account_id text NOT NULL,
    instrument_id text NOT NULL,
    observed_at timestamptz NOT NULL,
    document jsonb NOT NULL,
    created_at timestamptz NOT NULL DEFAULT clock_timestamp()
);
CREATE TABLE IF NOT EXISTS target.evidence_items (
    evidence_id text PRIMARY KEY,
    owner_id text NOT NULL,
    account_id text NOT NULL,
    canonical_url text NOT NULL,
    content_hash text NOT NULL,
    published_at timestamptz,
    fetched_at timestamptz NOT NULL,
    document jsonb NOT NULL,
    created_at timestamptz NOT NULL DEFAULT clock_timestamp()
);
CREATE TABLE IF NOT EXISTS target.selection_runs (
    run_id text PRIMARY KEY,
    owner_id text NOT NULL,
    account_id text NOT NULL,
    session_id text NOT NULL,
    selected_instrument_id text,
    document jsonb NOT NULL,
    created_at timestamptz NOT NULL DEFAULT clock_timestamp()
);
CREATE TABLE IF NOT EXISTS target.strategy_snapshots (
    snapshot_id text PRIMARY KEY,
    instrument_id text NOT NULL,
    formula_id text NOT NULL,
    document jsonb NOT NULL,
    created_at timestamptz NOT NULL DEFAULT clock_timestamp()
);
CREATE TABLE IF NOT EXISTS target.order_intents (
    intent_id text PRIMARY KEY,
    account_id text NOT NULL,
    instrument_id text NOT NULL,
    request_hash text NOT NULL,
    status text NOT NULL,
    document jsonb NOT NULL,
    created_at timestamptz NOT NULL DEFAULT clock_timestamp()
);
CREATE TABLE IF NOT EXISTS target.dynamic_exit_events (
    event_id text PRIMARY KEY,
    account_id text NOT NULL,
    plan_id text NOT NULL,
    reason text NOT NULL,
    document jsonb NOT NULL,
    created_at timestamptz NOT NULL DEFAULT clock_timestamp()
);
CREATE TABLE IF NOT EXISTS target.telegram_events (
    event_id text PRIMARY KEY,
    account_id text NOT NULL,
    chat_id bigint NOT NULL,
    document jsonb NOT NULL,
    created_at timestamptz NOT NULL DEFAULT clock_timestamp()
);
CREATE TABLE IF NOT EXISTS target.daily_reconciliation (
    report_id text PRIMARY KEY,
    account_id text NOT NULL,
    session_id text NOT NULL,
    complete boolean NOT NULL,
    document jsonb NOT NULL,
    created_at timestamptz NOT NULL DEFAULT clock_timestamp()
);
CREATE TABLE IF NOT EXISTS target.weekly_reports (
    report_id text PRIMARY KEY,
    account_id text NOT NULL,
    week_start timestamptz NOT NULL,
    week_end timestamptz NOT NULL,
    immutable_hash text NOT NULL CHECK (immutable_hash ~ '^[0-9a-f]{64}$'),
    document jsonb NOT NULL,
    created_at timestamptz NOT NULL DEFAULT clock_timestamp()
);

CREATE OR REPLACE FUNCTION target.append_record(
    p_kind text,
    p_record_id text,
    p_owner_id text,
    p_account_id text,
    p_document jsonb,
    p_content_sha256 text
) RETURNS boolean
LANGUAGE plpgsql SECURITY DEFINER SET search_path=target,public AS $$
DECLARE old jsonb;
BEGIN
    IF p_kind !~ '^[a-z][a-z0-9_]{1,63}$'
       OR p_record_id !~ '^[A-Za-z0-9][A-Za-z0-9_.:-]{0,159}$'
       OR p_owner_id !~ '^[A-Za-z0-9][A-Za-z0-9_.:-]{0,159}$'
       OR p_account_id !~ '^(paper|shadow|live)_[A-Za-z0-9_.:-]{1,127}$'
       OR p_content_sha256 !~ '^[0-9a-f]{64}$'
       OR p_document IS NULL THEN RAISE EXCEPTION 'INVALID_TARGET_RECORD'; END IF;
    SELECT document INTO old FROM target.records
      WHERE kind=p_kind AND record_id=p_record_id FOR UPDATE;
    IF old IS NOT NULL THEN
        IF old IS DISTINCT FROM p_document THEN RAISE EXCEPTION 'TARGET_RECORD_REBOUND'; END IF;
        RETURN false;
    END IF;
    INSERT INTO target.records(kind,record_id,owner_id,account_id,document,content_sha256)
      VALUES(p_kind,p_record_id,p_owner_id,p_account_id,p_document,p_content_sha256);
    RETURN true;
END $$;

REVOKE ALL ON FUNCTION target.append_record(text,text,text,text,jsonb,text) FROM PUBLIC;
GRANT USAGE ON SCHEMA target TO trady_analyst,trady_executor,trady_gateway;
GRANT EXECUTE ON FUNCTION target.append_record(text,text,text,text,jsonb,text) TO trady_analyst,trady_executor,trady_gateway;
