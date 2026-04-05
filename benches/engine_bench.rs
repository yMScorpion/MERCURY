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

use alloy::signers::local::PrivateKeySigner;
use alloy::signers::SignerSync;

fn bench_eip712_signature(c: &mut Criterion) {
    let wallet = PrivateKeySigner::random();
    
    // Criamos um hash simulado com bytes repetidos, dispensando a dependência de geradores aleatórios
    let hash = alloy::primitives::B256::repeat_byte(0x42);

    c.bench_function("eip712_sign_order", |b| {
        b.iter(|| {
            // Usamos a versão síncrona para medir o tempo puro de CPU da assinatura ECDSA
            let _ = black_box(wallet.sign_hash_sync(&hash));
        });
    });
}

// Registramos o novo benchmark criptográfico ao lado do seu teste existente
criterion_group!(benches, bench_spread_compute, bench_eip712_signature);
criterion_main!(benches);
