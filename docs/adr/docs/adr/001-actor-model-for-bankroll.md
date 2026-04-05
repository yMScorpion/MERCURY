# ADR 001: Actor Model for Bankroll

## Status
Accepted

## Context
The bankroll must be mutated by many concurrent tasks: the executor engine, settlement monitor, reconciler, and position tracker all need to read or update the bankroll state. Naive shared-state approaches using `Arc<Mutex<BankrollManager>>` create contention and risk deadlocks when tasks hold the lock across `.await` points.

## Decision
The `BankrollManager` is wrapped in a Tokio actor (`BankrollHandle`) that owns the state exclusively. All interactions go through an `mpsc::channel` of typed `BankrollMsg` messages. The actor processes messages sequentially, eliminating the need for any locks on the hot path.

## Consequences
- **Positive:** Zero lock contention; the actor serializes all mutations. Race conditions between `ReserveCapital` calls are naturally resolved (only one succeeds when capital is limited).
- **Positive:** Easy to reason about consistency — the actor is the single source of truth.
- **Negative:** All bankroll queries are async and require a round-trip through the channel. This adds a small but bounded latency (~microseconds on a local channel).
- **Negative:** If the actor task panics, all senders will eventually error. The main loop monitors the JoinSet for this.