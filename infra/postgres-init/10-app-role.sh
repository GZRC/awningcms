#!/bin/sh
# Runs once, when PostgreSQL creates its data directory: the application gets
# its own login role, without superuser rights, from its secret file.
set -eu
password=$(cat /run/secrets/app_db_password)
psql -v ON_ERROR_STOP=1 -v password="$password" \
    --username "$POSTGRES_USER" --dbname "$POSTGRES_DB" <<'SQL'
CREATE ROLE cms_app LOGIN NOSUPERUSER NOCREATEDB NOCREATEROLE NOREPLICATION PASSWORD :'password';
GRANT CONNECT ON DATABASE cms TO cms_app;
GRANT USAGE, CREATE ON SCHEMA public TO cms_app;
SQL
