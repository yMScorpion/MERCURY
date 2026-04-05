# ADR 003: SQLite with WAL Mode

## Status
Accepted

## Context
MERCURY is a single-instance engine running on one server. It needs a reliable, low-latency database for trade records, position tracking, and audit logs.

## Decision
SQLite in WAL (Write-Ahead Logging) mode is used instead of PostgreSQL or another server-based database.

## Consequences
- **Positive:** Zero operational overhead — no separate database process to manage, monitor, or back up separately.
- **Positive:** WAL mode provides concurrent reads with serialized writes, which matches MERCURY's access pattern (many readers, infrequent writers).
- **Positive:** The entire database is a single file, making backups trivial (`VACUUM INTO`).
- **Negative:** Not suitable for multi-instance deployments. If MERCURY ever scales horizontally, a migration to PostgreSQL would be required.
- **Negative:** SQLite's type system is weaker than PostgreSQL's (no native Decimal type, stored as TEXT).