use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

use chrono::Utc;
use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;

use ledger_service::domain::{
    pnl::StaticPricingEngine,
    portfolio::Portfolio,
    trade::{Trade, TradeSide},
};

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn buy(asset: &str, qty: Decimal, price: Decimal) -> Trade {
    Trade::new(asset, qty, price, Utc::now(), TradeSide::Buy).unwrap()
}

fn sell(asset: &str, qty: Decimal, price: Decimal) -> Trade {
    Trade::new(asset, qty, price, Utc::now(), TradeSide::Sell).unwrap()
}

// ─── 1. Sequential buy throughput ────────────────────────────────────────────

/// Measures how fast the portfolio can absorb consecutive buy trades for a
/// single asset (lot accumulation, no FIFO matching required).
fn bench_buy_sequential(c: &mut Criterion) {
    let mut group = c.benchmark_group("portfolio/buy_sequential");

    for n in [10usize, 100, 1_000] {
        group.bench_with_input(BenchmarkId::from_parameter(n), &n, |b, &n| {
            b.iter(|| {
                let mut portfolio = Portfolio::new();
                for i in 0..n {
                    let price = Decimal::from(30_000u64 + i as u64);
                    let trade = buy("BTC", dec!(1), price);
                    portfolio.apply_trade(black_box(&trade)).unwrap();
                }
                black_box(portfolio.total_realized_pnl)
            });
        });
    }

    group.finish();
}

// ─── 2. FIFO sell across N lots ──────────────────────────────────────────────

/// Builds a position with `n` equal lots of 1 unit each, then sells them all
/// in a single trade.  Exercises the hot path of `consume_fifo`.
fn bench_fifo_sell(c: &mut Criterion) {
    let mut group = c.benchmark_group("portfolio/fifo_sell");

    for n in [1usize, 10, 50, 200] {
        group.bench_with_input(BenchmarkId::from_parameter(n), &n, |b, &n| {
            b.iter_batched(
                || {
                    // Setup: n lots at ascending prices so each sell event
                    // has different cost basis.
                    let mut portfolio = Portfolio::new();
                    for i in 0..n {
                        let price = Decimal::from(100u64 + i as u64);
                        portfolio.apply_trade(&buy("BTC", dec!(1), price)).unwrap();
                    }
                    portfolio
                },
                |mut portfolio| {
                    // Sell all lots in one trade — touches every lot in the queue.
                    let qty = Decimal::from(n as u64);
                    let trade = sell("BTC", qty, dec!(500));
                    let events = portfolio.apply_trade(black_box(&trade)).unwrap();
                    black_box(events)
                },
                criterion::BatchSize::SmallInput,
            );
        });
    }

    group.finish();
}

// ─── 3. Unrealized PnL across N open positions ───────────────────────────────

/// Benchmarks `total_unrealized_pnl` with an increasing number of distinct
/// open asset positions.  Each call iterates every position, so complexity
/// is O(assets × lots).
fn bench_unrealized_pnl(c: &mut Criterion) {
    let mut group = c.benchmark_group("portfolio/unrealized_pnl");

    for n in [1usize, 10, 50, 200] {
        group.bench_with_input(BenchmarkId::from_parameter(n), &n, |b, &n| {
            // Build a portfolio with n assets, one lot each.
            let mut portfolio = Portfolio::new();
            let mut prices: HashMap<String, Decimal> = HashMap::new();

            for i in 0..n {
                let asset = format!("ASSET_{i}");
                let cost = Decimal::from(100u64 + i as u64);
                let current = Decimal::from(200u64 + i as u64);
                portfolio.apply_trade(&buy(&asset, dec!(10), cost)).unwrap();
                prices.insert(asset, current);
            }

            let engine = StaticPricingEngine::new(prices);

            b.iter(|| {
                let pnl = portfolio.total_unrealized_pnl(black_box(&engine)).unwrap();
                black_box(pnl)
            });
        });
    }

    group.finish();
}

// ─── 4. RwLock read contention ───────────────────────────────────────────────

/// Measures throughput when N threads all hold a concurrent read lock on the
/// same portfolio.  With `RwLock`, all readers proceed without blocking each
/// other; this benchmark validates that property and its scaling.
fn bench_rwlock_concurrent_reads(c: &mut Criterion) {
    let mut group = c.benchmark_group("portfolio/rwlock_concurrent_reads");

    // Pre-populate so there is meaningful work to do inside each read guard.
    let mut base = Portfolio::new();
    for i in 0..20 {
        let asset = format!("ASSET_{i}");
        base.apply_trade(&buy(&asset, dec!(10), Decimal::from(100u64 + i))).unwrap();
    }
    let shared = Arc::new(RwLock::new(base));

    for num_threads in [1usize, 2, 4, 8] {
        group.bench_with_input(
            BenchmarkId::from_parameter(num_threads),
            &num_threads,
            |b, &n| {
                b.iter(|| {
                    std::thread::scope(|s| {
                        let handles: Vec<_> = (0..n)
                            .map(|_| {
                                s.spawn(|| {
                                    let guard = shared.read().unwrap();
                                    // Simulate a read-only query (e.g. GET /portfolio).
                                    let total: Decimal = guard
                                        .positions()
                                        .values()
                                        .map(|p| p.total_quantity)
                                        .sum();
                                    black_box(total)
                                })
                            })
                            .collect();
                        for h in handles {
                            h.join().unwrap();
                        }
                    });
                });
            },
        );
    }

    group.finish();
}

// ─── 5. Write vs read contention (1 writer, N readers) ───────────────────────

/// Demonstrates that a single write lock does not starve waiting readers under
/// realistic workloads: one writer thread applies trades while reader threads
/// run concurrently.
fn bench_rwlock_write_read_contention(c: &mut Criterion) {
    let mut group = c.benchmark_group("portfolio/rwlock_write_read_contention");

    for num_readers in [0usize, 2, 4] {
        group.bench_with_input(
            BenchmarkId::from_parameter(num_readers),
            &num_readers,
            |b, &readers| {
                b.iter(|| {
                    let shared = Arc::new(RwLock::new(Portfolio::new()));

                    std::thread::scope(|s| {
                        // Spawn background readers.
                        for _ in 0..readers {
                            s.spawn(|| {
                                for _ in 0..50 {
                                    let guard = shared.read().unwrap();
                                    black_box(guard.total_realized_pnl);
                                }
                            });
                        }

                        // Writer: apply 50 buy trades.
                        for i in 0u64..50 {
                            let price = Decimal::from(30_000u64 + i);
                            let trade = buy("BTC", dec!(1), price);
                            shared.write().unwrap().apply_trade(black_box(&trade)).unwrap();
                        }
                    });
                });
            },
        );
    }

    group.finish();
}

// ─── Entry point ─────────────────────────────────────────────────────────────

criterion_group!(
    benches,
    bench_buy_sequential,
    bench_fifo_sell,
    bench_unrealized_pnl,
    bench_rwlock_concurrent_reads,
    bench_rwlock_write_read_contention,
);
criterion_main!(benches);
