//! Tests d'intégration : des programmes complets, vus de l'extérieur de la crate.

use vmrust::error::{Result, VmError};
use vmrust::interpreter::{ExecutionOutcome, Vm};
use vmrust::types::Gas;

/// Exécute du bytecode donné en hexadécimal (avec ou sans `0x`).
fn run_hex(hex_code: &str, gas_limit: u64) -> Result<ExecutionOutcome> {
    let code = hex::decode(hex_code.trim_start_matches("0x")).expect("hex invalide dans le test");
    Vm::new(&code, Gas::new(gas_limit)).run()
}

fn word(n: u8) -> Vec<u8> {
    let mut w = vec![0u8; 32];
    w[31] = n;
    w
}

// ---------- Programmes qui réussissent ----------

#[test]
fn program_1_add_and_return() {
    assert_eq!(
        run_hex("0x600260030160005260206000f3", 100_000).unwrap(),
        ExecutionOutcome::Success {
            return_data: word(5),
            gas_used: 24
        }
    );
}

#[test]
fn program_2_countdown_loop() {
    assert_eq!(
        run_hex("60035b600190038060025700", 100_000).unwrap(),
        ExecutionOutcome::Success {
            return_data: vec![],
            gas_used: 81
        }
    );
}

#[test]
fn loop_255_iterations_with_enough_gas() {
    // Même boucle avec un compteur de 255 : 3 + 255 * 26 = 6633 gas
    assert_eq!(
        run_hex("60ff5b600190038060025700", 100_000).unwrap(),
        ExecutionOutcome::Success {
            return_data: vec![],
            gas_used: 6_633
        }
    );
}

#[test]
fn computes_10_factorial_with_a_loop() {
    // Pile : [acc, n]. Tant que n != 0 : acc *= n ; n -= 1. Puis renvoie acc.
    // 00 PUSH1 1      acc = 1
    // 02 PUSH1 10     n = 10
    // 04 JUMPDEST
    // 05 DUP1         [acc, n, n]
    // 06 ISZERO       [acc, n, n==0]
    // 07 PUSH1 0x15
    // 09 JUMPI        si n == 0 -> fin
    // 0a DUP1         [acc, n, n]
    // 0b SWAP2        [n, n, acc]
    // 0c MUL          [n, acc*n]
    // 0d SWAP1        [acc*n, n]
    // 0e PUSH1 1
    // 10 SWAP1        [acc, 1, n]
    // 11 SUB          [acc, n-1]
    // 12 PUSH1 4
    // 14 JUMP
    // 15 JUMPDEST
    // 16 POP          [acc]
    // 17 PUSH1 0, MSTORE, PUSH1 32, PUSH1 0, RETURN
    let outcome = run_hex(
        concat!(
            "6001600a5b80156015578091029060019003600456",
            "5b50600052602060",
            "00f3"
        ),
        100_000,
    )
    .unwrap();
    let ExecutionOutcome::Success { return_data, .. } = outcome else {
        panic!("attendu Success, obtenu {outcome:?}");
    };
    let mut expected = [0u8; 32];
    expected[28..].copy_from_slice(&3_628_800u32.to_be_bytes());
    assert_eq!(return_data, expected);
}

#[test]
fn revert_with_empty_data() {
    assert_eq!(
        run_hex("60006000fd", 100_000).unwrap(),
        ExecutionOutcome::Revert {
            return_data: vec![],
            gas_used: 6
        }
    );
}

// ---------- Programmes qui échouent ----------

#[test]
fn add_on_empty_stack() {
    assert_eq!(
        run_hex("01", 100_000).unwrap_err(),
        VmError::StackUnderflow {
            needed: 2,
            available: 0
        }
    );
}

#[test]
fn add_with_a_single_item() {
    assert_eq!(
        run_hex("600101", 100_000).unwrap_err(),
        VmError::StackUnderflow {
            needed: 2,
            available: 1
        }
    );
}

#[test]
fn unknown_opcode() {
    assert_eq!(
        run_hex("0c", 100_000).unwrap_err(),
        VmError::InvalidOpcode(0x0c)
    );
}

#[test]
fn jump_outside_of_code() {
    assert_eq!(
        run_hex("600356", 100_000).unwrap_err(),
        VmError::InvalidJump(3)
    );
}

#[test]
fn jump_into_push_data() {
    assert_eq!(
        run_hex("605b600156", 100_000).unwrap_err(),
        VmError::InvalidJump(1)
    );
}

#[test]
fn infinite_loop_runs_out_of_gas() {
    assert!(matches!(
        run_hex("5b600056", 100_000).unwrap_err(),
        VmError::OutOfGas { .. }
    ));
}

#[test]
fn long_loop_runs_out_of_gas() {
    // 255 tours demandent 6633 gas, on n'en donne que 1000
    assert!(matches!(
        run_hex("60ff5b600190038060025700", 1_000).unwrap_err(),
        VmError::OutOfGas { .. }
    ));
}

#[test]
fn stack_overflow() {
    assert_eq!(
        run_hex(&"5f".repeat(1025), 100_000).unwrap_err(),
        VmError::StackOverflow { limit: 1024 }
    );
}
