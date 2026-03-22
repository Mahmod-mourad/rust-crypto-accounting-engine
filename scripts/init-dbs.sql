-- Initialise both service databases on first postgres startup.
-- PostgreSQL runs scripts in /docker-entrypoint-initdb.d/ only when the
-- data directory is empty (i.e. on a fresh volume).

CREATE DATABASE ledger_db;
CREATE DATABASE pnl_db;
