-- Run only against a disposable audit database after the synthetic imports.
-- These inspection queries are read-only. Imports and invalidation use the CLI.
BEGIN READ ONLY;

-- One retained observation for each of the nine persistable research features.
SELECT feature, count(*) AS observations
FROM research.intelligence_observations
GROUP BY feature
ORDER BY feature;

-- Tax observations must never enter shared research storage.
SELECT count(*) AS retained_tax_rows
FROM research.intelligence_observations
WHERE feature = 'TAX';

-- Rights revocation erases content while preserving permitted audit markers.
SELECT observation_id, feature, document IS NULL AS content_erased
FROM research.intelligence_observations
ORDER BY observation_id;

-- The analyst is the only listed role with SELECT on intelligence observations.
SELECT role_name,
       has_table_privilege(role_name, 'research.intelligence_observations', 'SELECT') AS can_select
FROM (VALUES ('trady_analyst'), ('trady_executor'), ('trady_gateway'),
             ('trady_shadow'), ('trady_lab'), ('trady_verifier')) AS roles(role_name)
ORDER BY role_name;

-- Inventory only the disposable database's application schemas.
SELECT table_schema, count(*) AS base_tables
FROM information_schema.tables
WHERE table_type = 'BASE TABLE'
  AND table_schema NOT IN ('pg_catalog', 'information_schema')
GROUP BY table_schema
ORDER BY table_schema;

ROLLBACK;
