-- Research observations and shared account credits; no broker permissions.
CREATE TABLE research.financial_accounts (
    budget_key text PRIMARY KEY,
    document jsonb NOT NULL
);
CREATE TABLE research.financial_sources (
    source_id text PRIMARY KEY CHECK (source_id ~ '^[A-Za-z0-9_-]{1,96}$'),
    registry_hash text NOT NULL,
    document jsonb NOT NULL,
    enabled boolean NOT NULL,
    updated_at timestamptz NOT NULL DEFAULT clock_timestamp()
);
CREATE TABLE research.provider_budget_windows (
    budget_key text NOT NULL,
    window_kind text NOT NULL CHECK (window_kind IN ('minute','day','month')),
    window_start timestamptz NOT NULL,
    used_credits bigint NOT NULL DEFAULT 0 CHECK (used_credits >= 0),
    credit_limit bigint NOT NULL CHECK (credit_limit > 0),
    PRIMARY KEY (budget_key, window_kind, window_start)
);
CREATE TABLE research.financial_registry_revisions (
    registry_hash text PRIMARY KEY,
    source_id text NOT NULL,
    document jsonb NOT NULL,
    recorded_at timestamptz NOT NULL DEFAULT clock_timestamp()
);
CREATE TABLE research.provider_retry_deadlines (
    budget_key text PRIMARY KEY,
    retry_at timestamptz NOT NULL
);
CREATE TABLE research.financial_collections (
    request_id text PRIMARY KEY,
    source_id text NOT NULL REFERENCES research.financial_sources,
    registry_hash text NOT NULL,
    window_document jsonb NOT NULL,
    status text NOT NULL,
    gaps jsonb NOT NULL DEFAULT '[]',
    started_at timestamptz NOT NULL,
    finished_at timestamptz,
    lease_until timestamptz NOT NULL,
    retry_at timestamptz,
    complete boolean NOT NULL DEFAULT false,
    reserved_credits integer NOT NULL CHECK (reserved_credits > 0)
);
CREATE INDEX financial_collections_recent ON research.financial_collections(source_id, started_at DESC, request_id);
CREATE TABLE research.financial_observations (
    observation_id text PRIMARY KEY,
    source_id text NOT NULL REFERENCES research.financial_sources,
    external_key text NOT NULL,
    content_hash text NOT NULL,
    registry_hash text NOT NULL,
    document jsonb NOT NULL,
    available_at timestamptz NOT NULL,
    received_at timestamptz NOT NULL,
    recorded_at timestamptz,
    expires_at timestamptz NOT NULL,
    CHECK (available_at <= received_at AND received_at < expires_at)
);
CREATE INDEX financial_observations_versions ON research.financial_observations(source_id, external_key, received_at DESC, observation_id);
CREATE TABLE research.financial_invalidations (
    observation_id text NOT NULL,
    replacement_id text NOT NULL,
    recorded_at timestamptz NOT NULL,
    PRIMARY KEY(observation_id, replacement_id)
);
CREATE TABLE research.financial_manifests (
    manifest_hash text PRIMARY KEY,
    source_id text NOT NULL REFERENCES research.financial_sources,
    decision_at timestamptz NOT NULL,
    metadata jsonb NOT NULL,
    observation_ids jsonb NOT NULL,
    created_at timestamptz NOT NULL,
    expires_at timestamptz NOT NULL
);
REVOKE ALL ON research.financial_accounts, research.financial_sources,
    research.provider_budget_windows, research.provider_retry_deadlines,
    research.financial_collections, research.financial_observations,
    research.financial_invalidations, research.financial_manifests FROM PUBLIC;
REVOKE ALL ON research.financial_registry_revisions FROM PUBLIC;
GRANT SELECT, INSERT, UPDATE ON research.financial_accounts, research.financial_sources,
    research.provider_budget_windows, research.provider_retry_deadlines,
    research.financial_collections TO trady_analyst;
GRANT SELECT, INSERT, DELETE ON research.financial_observations,
    research.financial_manifests TO trady_analyst;
-- Payloads stay immutable; availability is marked only after their commit.
GRANT UPDATE(recorded_at) ON research.financial_observations TO trady_analyst;
GRANT SELECT, INSERT ON research.financial_invalidations TO trady_analyst;
GRANT SELECT, INSERT ON research.financial_registry_revisions TO trady_analyst;
-- Consumer services go through the analyst's rights-aware application ports.
-- Executor, gateway, inference and lab roles cannot bypass those projections.
