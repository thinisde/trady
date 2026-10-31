-- Administrator-only, fresh Trady installation. Roles are server-wide.
-- Run against an isolated development server for tests. No passwords are stored here.
CREATE ROLE trady_owner NOLOGIN NOSUPERUSER NOCREATEDB NOCREATEROLE;
CREATE ROLE trady_migrator LOGIN NOINHERIT NOSUPERUSER NOCREATEDB NOCREATEROLE;
GRANT trady_owner TO trady_migrator;
CREATE ROLE trady_analyst LOGIN NOSUPERUSER NOCREATEDB NOCREATEROLE;
CREATE ROLE trady_executor LOGIN NOSUPERUSER NOCREATEDB NOCREATEROLE;
CREATE ROLE trady_gateway LOGIN NOSUPERUSER NOCREATEDB NOCREATEROLE;
