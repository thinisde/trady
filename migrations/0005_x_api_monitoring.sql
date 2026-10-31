-- Collector accounting only. No broker or executor grants.
CREATE TABLE research.x_budget_cycles (
    billing_scope text NOT NULL,
    cycle_start timestamptz NOT NULL,
    cycle_end timestamptz NOT NULL CHECK (cycle_end > cycle_start),
    policy_hash text NOT NULL,
    policy jsonb NOT NULL,
    provider_total_microusd bigint NOT NULL DEFAULT 0 CHECK (provider_total_microusd >= 0),
    provider_as_of timestamptz,
    blocked boolean NOT NULL DEFAULT false,
    PRIMARY KEY (billing_scope, cycle_start)
);
CREATE TABLE research.x_cost_reservations (
    reservation_id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    billing_scope text NOT NULL,
    cycle_start timestamptz NOT NULL,
    source_id text NOT NULL,
    registry_hash text NOT NULL,
    rule_hash text,
    resource_bound jsonb NOT NULL,
    actual_resources jsonb,
    ordinary_microusd bigint NOT NULL CHECK (ordinary_microusd >= 0),
    support_microusd bigint NOT NULL CHECK (support_microusd >= 0),
    outcome text NOT NULL CHECK (outcome IN ('RESERVED','OBSERVED','UNKNOWN_CONSUMPTION','RESOURCE_BOUND_EXCEEDED')),
    reserved_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    settled_at timestamptz,
    FOREIGN KEY (billing_scope, cycle_start) REFERENCES research.x_budget_cycles
);
CREATE INDEX x_cost_cycle ON research.x_cost_reservations(billing_scope,cycle_start,reserved_at);
CREATE TABLE research.x_cost_reconciliations (
    reconciliation_id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    billing_scope text NOT NULL,
    cycle_start timestamptz NOT NULL,
    provider_total_microusd bigint NOT NULL CHECK (provider_total_microusd >= 0),
    provider_as_of timestamptz NOT NULL,
    evidence_reference text NOT NULL,
    recorded_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    FOREIGN KEY (billing_scope, cycle_start) REFERENCES research.x_budget_cycles
);

-- Availability stays unknown until the collector records it after the content transaction commits.
ALTER TABLE research.source_items ADD COLUMN ingested_at timestamptz;
ALTER TABLE research.source_items ADD COLUMN ingestion_revision bigint;
ALTER TABLE research.source_items ADD COLUMN registry_hash text;
ALTER TABLE research.source_items ADD COLUMN coverage jsonb;
CREATE FUNCTION research.touch_source_ingestion() RETURNS trigger
LANGUAGE plpgsql SET search_path = pg_catalog AS $$
BEGIN
    IF (to_jsonb(NEW) - 'ingested_at') IS DISTINCT FROM (to_jsonb(OLD) - 'ingested_at') THEN
        NEW.ingested_at := NULL;
    END IF;
    RETURN NEW;
END
$$;
CREATE TRIGGER source_ingestion BEFORE UPDATE ON research.source_items
FOR EACH ROW EXECUTE FUNCTION research.touch_source_ingestion();

REVOKE ALL ON research.x_budget_cycles, research.x_cost_reservations, research.x_cost_reconciliations FROM PUBLIC;
REVOKE ALL ON FUNCTION research.touch_source_ingestion() FROM PUBLIC;
GRANT SELECT, INSERT, UPDATE ON research.x_budget_cycles, research.x_cost_reservations TO trady_analyst;
GRANT SELECT, INSERT ON research.x_cost_reconciliations TO trady_analyst;
GRANT USAGE ON SEQUENCE research.x_cost_reservations_reservation_id_seq, research.x_cost_reconciliations_reconciliation_id_seq TO trady_analyst;
