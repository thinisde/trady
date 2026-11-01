-- Optional research only. No executor/gateway/inference/lab grants, no tax data.
CREATE TABLE research.intelligence_registries (
    registry_hash text PRIMARY KEY CHECK (registry_hash ~ '^[a-f0-9]{64}$'),
    document jsonb NOT NULL,
    recorded_at timestamptz NOT NULL DEFAULT clock_timestamp()
);
CREATE TABLE research.intelligence_observations (
    owner_id text NOT NULL CHECK (owner_id ~ '^[A-Za-z0-9_-]{1,96}$'),
    account_id text NOT NULL CHECK (account_id ~ '^[A-Za-z0-9_-]{1,96}$'),
    observation_id text NOT NULL CHECK (observation_id ~ '^[A-Za-z0-9_-]{1,96}$'),
    feature text NOT NULL CHECK (feature IN ('OPTIONS','COORDINATION','EVENT','SHORT_INTEREST','INSIDER','CROSS_ASSET','ATTENTION','ACCOUNTING','MICROSTRUCTURE')),
    entity_id text NOT NULL,
    content_hash text NOT NULL CHECK (content_hash ~ '^[a-f0-9]{64}$'),
    registry_hash text NOT NULL REFERENCES research.intelligence_registries,
    ingested_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    retain_until timestamptz NOT NULL,
    source_ids jsonb NOT NULL,
    lineage_ids jsonb NOT NULL,
    document jsonb,
    CHECK (document IS NULL OR document->'payload'->>'feature'=feature),
    PRIMARY KEY(owner_id,account_id,observation_id)
);
CREATE INDEX intelligence_observations_lookup ON research.intelligence_observations
    (owner_id,account_id,feature,entity_id,ingested_at DESC,observation_id);
CREATE FUNCTION research.guard_intelligence_erasure() RETURNS trigger
LANGUAGE plpgsql SET search_path=pg_catalog,pg_temp AS $$
BEGIN
    IF NEW.document IS NOT NULL OR OLD.document IS NULL THEN
        RAISE EXCEPTION 'intelligence documents are immutable except erasure';
    END IF;
    RETURN NEW;
END;
$$;
CREATE TRIGGER intelligence_erasure_only BEFORE UPDATE ON research.intelligence_observations
FOR EACH ROW EXECUTE FUNCTION research.guard_intelligence_erasure();
REVOKE ALL ON FUNCTION research.guard_intelligence_erasure() FROM PUBLIC;
CREATE TABLE research.intelligence_invalidations (
    invalidation_id text PRIMARY KEY CHECK (invalidation_id ~ '^[a-f0-9]{64}$'),
    owner_id text NOT NULL,
    account_id text NOT NULL,
    target_kind text NOT NULL CHECK (target_kind IN ('OBSERVATION','SOURCE','LINEAGE')),
    target_id text NOT NULL,
    invalidated_at timestamptz NOT NULL,
    erase_content boolean NOT NULL,
    document jsonb NOT NULL
);
CREATE INDEX intelligence_invalidations_scope ON research.intelligence_invalidations(owner_id,account_id,invalidated_at);
CREATE TABLE research.intelligence_assessments (
    assessment_id text PRIMARY KEY,
    owner_id text NOT NULL,
    account_id text NOT NULL,
    observation_id text NOT NULL,
    source_ids jsonb NOT NULL,
    lineage_ids jsonb NOT NULL,
    retain_until timestamptz NOT NULL,
    document jsonb NOT NULL CHECK (document->>'feature'<>'TAX' AND document->>'entry_authorized'='false'),
    FOREIGN KEY(owner_id,account_id,observation_id) REFERENCES research.intelligence_observations
);
CREATE TABLE research.intelligence_rejections (
    rejection_id text PRIMARY KEY CHECK (rejection_id ~ '^[a-f0-9]{64}$'),
    owner_id text NOT NULL CHECK (owner_id ~ '^[A-Za-z0-9_-]{1,96}$'),
    account_id text NOT NULL CHECK (account_id ~ '^[A-Za-z0-9_-]{1,96}$'),
    observation_id text CHECK (observation_id ~ '^[A-Za-z0-9_-]{1,96}$'),
    source_ids jsonb NOT NULL,
    lineage_ids jsonb NOT NULL,
    retain_until timestamptz NOT NULL,
    document jsonb NOT NULL,
    rejected_at timestamptz NOT NULL DEFAULT clock_timestamp()
);
REVOKE ALL ON research.intelligence_registries, research.intelligence_observations,
    research.intelligence_invalidations, research.intelligence_assessments,
    research.intelligence_rejections FROM PUBLIC;
GRANT SELECT,INSERT ON research.intelligence_registries, research.intelligence_invalidations TO trady_analyst;
GRANT SELECT,INSERT,DELETE ON research.intelligence_rejections TO trady_analyst;
GRANT SELECT,INSERT ON research.intelligence_observations TO trady_analyst;
-- Input documents may only be erased. Every non-content revision remains immutable.
GRANT UPDATE(document) ON research.intelligence_observations TO trady_analyst;
GRANT SELECT,INSERT,DELETE ON research.intelligence_assessments TO trady_analyst;
