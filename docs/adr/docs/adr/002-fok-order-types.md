# ADR 002: Fill-or-Kill Order Types

## Status
Accepted

## Context
Arbitrage requires that both legs of a trade fill at approximately the same time. A partial fill on one leg creates a naked position: unhedged exposure that can result in significant losses if the market moves before the hedge executes.

## Decision
All orders submitted by MERCURY use Fill-or-Kill (FOK) time-in-force. FOK orders either fill completely at the specified price or are rejected entirely. This makes execution binary: both legs fill, or neither does (triggering the unwind path).

## Consequences
- **Positive:** No partial fill risk. The executor's logic can assume either full fill or zero fill.
- **Positive:** Simplifies position accounting — no need to track partial fills across multiple order IDs.
- **Negative:** Lower fill rate. In thin markets, FOK orders are more likely to be rejected than IOC (Immediate-or-Cancel).
- **Negative:** Requires careful sizing — if the requested size exceeds available liquidity, the order will always fail.