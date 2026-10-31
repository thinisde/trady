-- Session state has no authority over financial tables or executor queues.
CREATE TABLE research.model_sessions (
    session_id text PRIMARY KEY CHECK (session_id ~ '^session_[a-f0-9]{32}$'),
    owner_id text NOT NULL CHECK (owner_id ~ '^[A-Za-z0-9_-]{1,96}$'),
    account_id text NOT NULL CHECK (account_id ~ '^paper_[A-Za-z0-9_-]+$'),
    policy_version integer NOT NULL CHECK (policy_version > 0),
    revision bigint NOT NULL DEFAULT 0 CHECK (revision >= 0),
    document jsonb NOT NULL CHECK (jsonb_typeof(document) = 'object'),
    lease_worker text,
    lease_generation bigint NOT NULL DEFAULT 0 CHECK (lease_generation >= 0),
    lease_until timestamptz NOT NULL DEFAULT '-infinity',
    created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    updated_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    CHECK ((document->>'schema_version')::integer = 1),
    CHECK (document->>'id' = session_id),
    CHECK (document->'scope'->>'owner_id' = owner_id),
    CHECK (document->'scope'->>'account_id' = account_id),
    CHECK ((document->'scope'->>'policy_version')::integer = policy_version),
    CHECK ((document->>'revision')::bigint = revision),
    CHECK (document ?& ARRAY['schema_version','id','scope','revision']),
    CHECK (document->'scope' ?& ARRAY['owner_id','account_id','policy_version'])
);
CREATE INDEX model_sessions_owner_account ON research.model_sessions(owner_id,account_id,policy_version,updated_at DESC);
REVOKE ALL ON research.model_sessions FROM PUBLIC;
GRANT SELECT,INSERT,UPDATE ON research.model_sessions TO trady_analyst;
-- Executor and gateway cannot read transcripts, acquire generation leases or change sessions.
