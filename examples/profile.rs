//! Programme à profiler : exécute en boucle le compte à rebours de 10 000 tours.
//!
//! Flamegraph :
//! - sans droits root : `./scripts/flamegraph.sh flamegraph.svg`
//! - avec cargo-flamegraph (dtrace, demande le mot de passe sur macOS) :
//!   `cargo flamegraph --root --example profile`
//!
//! Le nombre d'exécutions peut être passé en argument (20 000 par défaut).

use std::hint::black_box;

use vmrust::interpreter::Vm;
use vmrust::types::Gas;

/// Même boucle que dans `benches/interpreter.rs`.
fn countdown(n: u16) -> Vec<u8> {
    let [hi, lo] = n.to_be_bytes();
    vec![
        0x61, hi, lo, 0x5b, 0x60, 0x01, 0x90, 0x03, 0x80, 0x60, 0x03, 0x57, 0x00,
    ]
}

fn main() {
    let iterations: usize = std::env::args()
        .nth(1)
        .and_then(|arg| arg.parse().ok())
        .unwrap_or(20_000);
    let code = countdown(10_000);

    let start = std::time::Instant::now();
    for _ in 0..iterations {
        black_box(Vm::new(black_box(&code), Gas::new(u64::MAX)).run().unwrap());
    }
    println!("{iterations} exécutions en {:?}", start.elapsed());
}
