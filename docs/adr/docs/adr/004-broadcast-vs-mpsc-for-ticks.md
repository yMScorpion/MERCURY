# ADR 004: Broadcast Channel for Tick Distribution

## Status
Accepted (with known trade-offs)

## Context
Market ticks from each feed need to be distributed to the relevant `MarketActor` tasks. The naive approach routes every tick through the main loop, which then dispatches to per-market `mpsc` channels.

## Decision
A `tokio::sync::broadcast` channel with a 10,000-message buffer is used for the global tick stream. The main event loop subscribes to this channel and routes ticks to per-market `mpsc` channels for the `MarketActor` tasks.

## Consequences
- **Positive:** Decouples the feed handlers from the routing logic. Feed handlers just send to the broadcast channel without knowing about market actors.
- **Positive:** Easy to add new subscribers (e.g., a metrics collector) without modifying feed code.
- **Negative:** `broadcast::Receiver::recv()` returns `Lagged(n)` when the buffer fills up, dropping `n` ticks. The detector is paused for 2 seconds on lag events to avoid acting on stale books.
- **Negative:** All ticks go through the main loop, which is a single-threaded bottleneck. Per-market `mpsc` channels handle the actual per-actor routing.