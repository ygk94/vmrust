//! Démo en ligne de commande de la mini-EVM.
//!
//! - `cargo run` : exécute une série de programmes d'exemple ;
//! - `cargo run -- <bytecode hex> [gas]` : exécute ton propre bytecode.
//!   Exemple : `cargo run -- 600260030160005260206000f3` ;
//! - `cargo run -- trace <bytecode hex> [gas]` : pareil, pas à pas ;
//! - `cargo run -- disasm <bytecode hex>` : affiche les instructions ;
//! - `cargo run -- serve [adresse:port]` : lance l'API HTTP (127.0.0.1:3000 par défaut).

use vmrust::disasm::disassemble;
use vmrust::interpreter::{ExecutionOutcome, Vm};
use vmrust::types::Gas;

const DEFAULT_GAS: u64 = 100_000;
const DEFAULT_LISTEN: &str = "127.0.0.1:3000";

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
    let args: Vec<&str> = args.iter().map(String::as_str).collect();

    match args.as_slice() {
        [] => {
            println!("=== Démo mini-EVM ===\n");
            for &(description, hex_code, gas) in DEMOS {
                execute(description, hex_code, gas, false);
            }
        }
        ["serve", rest @ ..] => serve(rest.first().unwrap_or(&DEFAULT_LISTEN)),
        ["trace" | "disasm"] => eprintln!("il manque le bytecode hexadécimal"),
        ["trace", hex_code, rest @ ..] => execute("trace", hex_code, parse_gas(rest), true),
        ["disasm", hex_code, ..] => {
            if let Some(code) = decode(hex_code) {
                for instruction in disassemble(&code) {
                    println!("{instruction}");
                }
            }
        }
        [hex_code, rest @ ..] => execute("ton bytecode", hex_code, parse_gas(rest), false),
    }
}

fn parse_gas(args: &[&str]) -> u64 {
    args.first()
        .and_then(|g| g.parse().ok())
        .unwrap_or(DEFAULT_GAS)
}

fn serve(listen: &str) {
    let result = tokio::runtime::Runtime::new().and_then(|runtime| {
        runtime.block_on(async {
            let listener = tokio::net::TcpListener::bind(listen).await?;
            println!("API sur http://{}", listener.local_addr()?);
            axum::serve(listener, vmrust::api::router()).await
        })
    });
    if let Err(e) = result {
        eprintln!("impossible de lancer le serveur sur {listen} : {e}");
        std::process::exit(1);
    }
}

/// Décode le bytecode hexadécimal, ou affiche pourquoi c'est impossible.
fn decode(hex_code: &str) -> Option<Vec<u8>> {
    let cleaned: String = hex_code
        .trim_start_matches("0x")
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    hex::decode(cleaned)
        .inspect_err(|e| eprintln!("bytecode hexadécimal invalide : {e}"))
        .ok()
}

/// Affiche une ligne par instruction : position, nom, gas restant, pile.
fn print_step(vm: &Vm) {
    let name = vm
        .current_opcode()
        .map_or_else(|| "-".to_string(), |op| op.to_string());
    let stack: Vec<String> = vm
        .stack()
        .as_slice()
        .iter()
        .map(|w| w.to_string())
        .collect();
    println!(
        "  {:04x}  {name:<10} gas {:<8} pile [{}]",
        vm.pc(),
        vm.gas_remaining().value(),
        stack.join(", ")
    );
}

fn execute(description: &str, hex_code: &str, gas_limit: u64, trace: bool) {
    let Some(code) = decode(hex_code) else {
        return;
    };

    println!("▶ {description}");
    println!(
        "  bytecode : {}   (gas donné : {gas_limit})",
        hex::encode(&code)
    );

    let mut vm = Vm::new(&code, Gas::new(gas_limit));
    let result = if trace {
        vm.run_with(print_step)
    } else {
        vm.run()
    };
    match result {
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
