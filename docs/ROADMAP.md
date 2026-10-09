# MERCURY delivery roadmap



| Stage | Implemented | Still required |
|---|---|---|
| Data ingestion | Four adapter families, discovery and normalization | Controlled connectivity/contract tests for each venue |
| Detection | Unified book and net-cost evaluation | Validate market equivalence and cost calibration |
| Risk | Exposure/loss/staleness breakers and sizing | Independent invariant review and restart/recovery scenarios |
| Execution | Dry-run/client interfaces, dedup/backoff, partial failure paths | Sandbox evidence for both legs and unwind/reconciliation |
| Inventory | SQLite persistence, settlement and watchdog paths | Long-running failure injection and reconciliation reports |
| Operations | Health, metrics, Telegram alert/report formats | Repeatable deployment, measured service objectives and optional dashboard |

[Detailed acceptance criteria](ROADMAP.md). The repository is a work in progress; avoid interpreting a version label as operational readiness.


## Release gate

A production-ready claim requires reproducible deployment, recovery tests, security boundaries and measured runtime behavior; source inspection alone is not sufficient.
