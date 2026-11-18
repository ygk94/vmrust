//! Benchmarks de l'interpréteur avec criterion.
//!
//! Lancer : `cargo bench`, puis ouvrir `target/criterion/report/index.html`.

use std::hint::black_box;

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use vmrust::interpreter::Vm;
use vmrust::types::Gas;

/// Compte à rebours de `n` à 0 (boucle du programme 2, compteur sur 2 octets).
///
/// 00 PUSH2 n | 03 JUMPDEST | PUSH1 1 | SWAP1 | SUB | DUP1 | PUSH1 3 | JUMPI | STOP
fn countdown(n: u16) -> Vec<u8> {
    let [hi, lo] = n.to_be_bytes();
    vec![
        0x61, hi, lo, 0x5b, 0x60, 0x01, 0x90, 0x03, 0x80, 0x60, 0x03, 0x57, 0x00,
    ]
}

/// Boucle de `n` tours qui écrit puis relit le compteur en mémoire à chaque tour.
///
/// 00 PUSH2 n
/// 03 JUMPDEST
/// 04 DUP1 PUSH1 0 MSTORE        memory[0] = c
/// 08 PUSH1 0 MLOAD              [c, c]
/// 0b PUSH1 1 SWAP1 SUB          [c, c-1]
/// 0f SWAP1 POP                  [c-1]
/// 11 DUP1 PUSH1 3 JUMPI
/// 15 STOP
fn memory_loop(n: u16) -> Vec<u8> {
    let [hi, lo] = n.to_be_bytes();
    vec![
        0x61, hi, lo, 0x5b, 0x80, 0x60, 0x00, 0x52, 0x60, 0x00, 0x51, 0x60, 0x01, 0x90, 0x03, 0x90,
        0x50, 0x80, 0x60, 0x03, 0x57, 0x00,
    ]
}

fn execute(code: &[u8]) {
    let outcome = Vm::new(black_box(code), Gas::new(u64::MAX)).run().unwrap();
    black_box(outcome);
}

fn bench_small_programs(c: &mut Criterion) {
    // (2 + 3) * 4
    let arith = hex::decode("600260030160040200").unwrap();
    // Programme 1 du guide : 2 + 3 écrit en mémoire puis renvoyé
    let program_1 = hex::decode("600260030160005260206000f3").unwrap();

    c.bench_function("arith_small", |b| b.iter(|| execute(&arith)));
    c.bench_function("program_1_return", |b| b.iter(|| execute(&program_1)));
}

fn bench_loops(c: &mut Criterion) {
    let mut group = c.benchmark_group("countdown");
    for n in [100u16, 1_000, 10_000] {
        let code = countdown(n);
        // 7 instructions par tour : criterion affiche aussi le débit en instructions/s
        group.throughput(Throughput::Elements(7 * n as u64));
        group.bench_with_input(BenchmarkId::from_parameter(n), &code, |b, code| {
            b.iter(|| execute(code))
        });
    }
    group.finish();

    let code = memory_loop(10_000);
    c.bench_function("memory_loop_10k", |b| b.iter(|| execute(&code)));
}

criterion_group!(benches, bench_small_programs, bench_loops);
criterion_main!(benches);
