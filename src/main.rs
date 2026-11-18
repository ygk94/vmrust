//! Démo en ligne de commande de la mini-EVM.
//!
//! - `cargo run` : exécute une série de programmes d'exemple ;
//! - `cargo run -- <bytecode hex> [gas]` : exécute ton propre bytecode.
//!   Exemple : `cargo run -- 600260030160005260206000f3`

use vmrust::interpreter::{ExecutionOutcome, Vm};
use vmrust::types::Gas;

const DEFAULT_GAS: u64 = 100_000;

/// Programmes de démonstration : (description, bytecode hex, gas donné).
const DEMOS: &[(&str, &str, u64)] = &[
    (
        "2 + 3, écrit en mémoire puis renvoyé",
        "600260030160005260206000f3",
        DEFAULT_GAS,
    ),
    (
        "10! (factorielle) calculée avec une boucle",
        "6001600a5b801560155780910290600190036004565b5060005260206000f3",
        DEFAULT_GAS,
    ),
    (
        "boucle de compte à rebours 3 -> 0",
        "60035b600190038060025700",
        DEFAULT_GAS,
    ),
    ("REVERT volontaire", "60006000fd", DEFAULT_GAS),
    ("ADD sur une pile vide", "01", DEFAULT_GAS),
    ("octet qui n'est pas un opcode", "0c", DEFAULT_GAS),
    (
        "saut au milieu d'une donnée de PUSH",
        "605b600156",
        DEFAULT_GAS,
    ),
    ("boucle infinie, arrêtée par le gas", "5b600056", 10_000),
];

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();

    if let Some(hex_code) = args.first() {
        let gas = args
            .get(1)
            .and_then(|g| g.parse().ok())
            .unwrap_or(DEFAULT_GAS);
        execute("ton bytecode", hex_code, gas);
        return;
    }

    println!("=== Démo mini-EVM ===\n");
    for &(description, hex_code, gas) in DEMOS {
        execute(description, hex_code, gas);
    }
}

fn execute(description: &str, hex_code: &str, gas_limit: u64) {
    let cleaned: String = hex_code
        .trim_start_matches("0x")
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    let code = match hex::decode(&cleaned) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("bytecode hexadécimal invalide : {e}");
            return;
        }
    };

    println!("▶ {description}");
    println!("  bytecode : {cleaned}   (gas donné : {gas_limit})");

    match Vm::new(&code, Gas::new(gas_limit)).run() {
        Ok(ExecutionOutcome::Success {
            return_data,
            gas_used,
        }) => println!(
            "  ✅ succès   gas utilisé : {gas_used}{}",
            describe_data(&return_data)
        ),
        Ok(ExecutionOutcome::Revert {
            return_data,
            gas_used,
        }) => println!(
            "  ↩️  revert   gas utilisé : {gas_used}{}",
            describe_data(&return_data)
        ),
        Err(error) => println!("  ❌ erreur   {error}   (tout le gas est perdu)"),
    }
    println!();
}

/// Affiche les données renvoyées en hex, et en nombre si elles tiennent sur 32 octets.
fn describe_data(data: &[u8]) -> String {
    if data.is_empty() {
        return String::new();
    }
    let mut text = format!("\n  données renvoyées : 0x{}", hex::encode(data));
    if data.len() <= 32 {
        let value = vmrust::types::Word::from_be_slice(data);
        text.push_str(&format!("\n  en décimal : {value}"));
    }
    text
}
