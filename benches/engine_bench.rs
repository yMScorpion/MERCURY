use criterion::{black_box, criterion_group, criterion_main, Criterion};
use mercury::engine::spread::NetSpreadEngine;
use mercury::engine::order_book::PlatformBook;
use mercury::types::*;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use uuid::Uuid;

fn bench_spread_compute(c: &mut Criterion) {
    let engine = NetSpreadEngine::new(dec!(0.001));
    let market_id = Uuid::new_v4();
    
    let mut book_a = PlatformBook::new(Platform::Kalshi, market_id);
    for i in 1..10 {
        book_a.asks.insert(dec!(0.5) + Decimal::from(i) * dec!(0.01), dec!(100));
    }
    
    let mut book_b = PlatformBook::new(Platform::Cdna, market_id);
    for i in 1..10 {
        book_b.bids.insert(dec!(0.5) - Decimal::from(i) * dec!(0.01), dec!(100));
    }

    c.bench_function("compute_spreads", |b| {
        b.iter(|| {
            engine.compute_spreads(black_box(&book_a), black_box(&book_b), black_box(dec!(100)))
        })
    });
}

criterion_group!(benches, bench_spread_compute);
criterion_main!(benches);
