-- Read-only provider observations; these grants confer no execution authority.
CREATE TABLE research.source_states (
    source_id text PRIMARY KEY CHECK (source_id ~ '^[A-Za-z0-9_-]{1,96}$'),
    revision bigint NOT NULL DEFAULT 0 CHECK (revision >= 0),
    document jsonb NOT NULL,
    updated_at timestamptz NOT NULL DEFAULT clock_timestamp()
);
CREATE TABLE research.source_items (
    item_id text PRIMARY KEY,
    source_id text NOT NULL REFERENCES research.source_states,
    external_id text NOT NULL,
    lineage_id text NOT NULL,
    content_hash text NOT NULL,
    document jsonb NOT NULL,
    first_seen_at timestamptz NOT NULL,
    fetched_at timestamptz NOT NULL,
    expires_at timestamptz NOT NULL,
    UNIQUE (source_id, external_id)
);
CREATE INDEX source_items_recent ON research.source_items (fetched_at DESC, item_id);
CREATE INDEX source_items_lineage ON research.source_items (lineage_id);
CREATE INDEX source_items_external ON research.source_items (external_id);
CREATE TABLE research.source_item_revisions (
    item_id text NOT NULL REFERENCES research.source_items,
    content_hash text NOT NULL,
    document jsonb NOT NULL,
    expires_at timestamptz NOT NULL,
    PRIMARY KEY (item_id, content_hash)
);
CREATE TABLE research.collection_events (
    event_id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    source_id text NOT NULL REFERENCES research.source_states,
    revision bigint NOT NULL,
    status text NOT NULL CHECK (status IN ('COMPLETE','PARTIAL','UNAVAILABLE','RATE_LIMITED','DISABLED')),
    document jsonb NOT NULL,
    recorded_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    UNIQUE (source_id, revision)
);
CREATE TABLE research.source_invalidations (
    item_id text NOT NULL REFERENCES research.source_items,
    replacement_hash text NOT NULL,
    reason text NOT NULL CHECK (reason IN ('CONTENT_CHANGED','SUPERSEDED','UNAVAILABLE')),
    recorded_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    PRIMARY KEY (item_id, replacement_hash, reason)
);

-- Remove source content, including expanded parent/quoted text, retaining audit metadata.
CREATE FUNCTION research.redact_item(doc jsonb) RETURNS jsonb
LANGUAGE sql IMMUTABLE SET search_path = pg_catalog AS $$
    SELECT (doc - 'feed_title' - 'title' - 'summary' - 'text') || jsonb_build_object(
        'content_status', CASE WHEN doc->>'content_status' IN ('SUPERSEDED','UNAVAILABLE')
            THEN doc->>'content_status' ELSE 'METADATA_ONLY' END,
        'social', CASE WHEN jsonb_typeof(doc->'social') = 'object' THEN
            (doc->'social') || jsonb_build_object('context', COALESCE((
                SELECT jsonb_agg(value - 'text') FROM jsonb_array_elements(doc->'social'->'context')
            ), '[]'::jsonb)) ELSE 'null'::jsonb END
    )
$$;
REVOKE ALL ON ALL TABLES IN SCHEMA research FROM PUBLIC;
REVOKE ALL ON FUNCTION research.redact_item(jsonb) FROM PUBLIC;
GRANT SELECT, INSERT, UPDATE ON research.source_states, research.source_items TO trady_analyst;
GRANT SELECT, INSERT, UPDATE ON research.source_item_revisions TO trady_analyst;
GRANT SELECT, INSERT ON research.collection_events, research.source_invalidations TO trady_analyst;
GRANT USAGE ON SEQUENCE research.collection_events_event_id_seq TO trady_analyst;
GRANT EXECUTE ON FUNCTION research.redact_item(jsonb) TO trady_analyst;
