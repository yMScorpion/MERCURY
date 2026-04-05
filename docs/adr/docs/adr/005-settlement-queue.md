# ADR 005: Persistent Settlement Queue

## Status
Accepted

## Context
When a market resolves, the `SettlementMonitor` needs to update the `BankrollActor` with realized PnL. If the actor's channel is full (backpressure), the settlement could be lost on process restart.

## Decision
Settlements are written to the `pending_settlements` SQLite table before being sent to the `BankrollActor`. The `SettlementMonitor` drains this queue every 60 seconds, retrying any settlements that failed to send due to channel backpressure.

## Consequences
- **Positive:** No PnL is lost on crash or channel saturation. The DB is the durable record.
- **Positive:** The settlement queue provides natural backpressure — if the actor is overwhelmed, the monitor backs off and retries.
- **Negative:** Settlements may be delayed by up to 60 seconds if the channel is consistently full.
- **Negative:** Adds a database round-trip on every settlement, which is acceptable given settlement frequency (typically a few per day).