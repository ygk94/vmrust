//! Démo en ligne de commande de la mini-EVM.
//!
//! - `cargo run` : exécute une série de programmes d'exemple ;
//! - `cargo run -- <bytecode hex> [gas] [calldata hex]` : exécute ton propre bytecode.
//!   Exemple : `cargo run -- 600260030160005260206000f3`. Un calldata qui commence par
//!   0x peut être donné sans le gas ;
//! - `cargo run -- trace <bytecode hex> [gas] [calldata hex]` : pareil, pas à pas ;
//! - `cargo run -- disasm <bytecode hex>` : affiche les instructions ;
//! - `cargo run -- serve [adresse:port]` : lance l'API HTTP (127.0.0.1:3000 par défaut).

use vmrust::disasm::disassemble;
use vmrust::interpreter::{ExecutionOutcome, Vm};
use vmrust::types::{Gas, Word};

const DEFAULT_GAS: u64 = 100_000;
const DEFAULT_LISTEN: &str = "127.0.0.1:3000";

/// Programmes de démonstration : (description, bytecode hex, calldata hex, gas donné).
const DEMOS: &[(&str, &str, &str, u64)] = &[
    (
        "2 + 3, écrit en mémoire puis renvoyé",
        "600260030160005260206000f3",
        "",
        DEFAULT_GAS,
    ),
    (
        "2 + 3, avec les deux nombres passés en calldata",
        "5f35602035015f5260205ff3",
        concat!(
            "0000000000000000000000000000000000000000000000000000000000000002",
            "0000000000000000000000000000000000000000000000000000000000000003",
        ),
        DEFAULT_GAS,
    ),
    (
        "10! (factorielle) calculée avec une boucle",
        "6001600a5b801560155780910290600190036004565b5060005260206000f3",
        "",
        DEFAULT_GAS,
    ),
    (
        "boucle de compte à rebours 3 -> 0",
        "60035b600190038060025700",
        "",
        DEFAULT_GAS,
    ),
    (
        "storage : écrit 42 dans la case 1 puis la relit",
        "602a6001556001545f5260205ff3",
        "",
        DEFAULT_GAS,
    ),
    (
        "keccak256 du texte \"abc\"",
        "6261626360e81b5f5260035f205f5260205ff3",
        "",
        DEFAULT_GAS,
    ),
    ("REVERT volontaire", "60006000fd", "", DEFAULT_GAS),
    ("ADD sur une pile vide", "01", "", DEFAULT_GAS),
    ("octet qui n'est pas un opcode", "0c", "", DEFAULT_GAS),
    (
        "saut au milieu d'une donnée de PUSH",
        "605b600156",
        "",
        DEFAULT_GAS,
    ),
    ("boucle infinie, arrêtée par le gas", "5b600056", "", 10_000),
];

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();

    match args.as_slice() {
        [] => {
            println!("=== Démo mini-EVM ===\n");
            for &(description, hex_code, calldata, gas) in DEMOS {
                execute(description, hex_code, calldata, gas, false);
            }
        }
        ["serve", rest @ ..] => serve(rest.first().unwrap_or(&DEFAULT_LISTEN)),
        ["trace" | "disasm"] => eprintln!("il manque le bytecode hexadécimal"),
        ["trace", hex_code, rest @ ..] => execute_args("trace", hex_code, rest, true),
        ["disasm", hex_code, ..] => {
            if let Some(code) = decode("bytecode", hex_code) {
                for instruction in disassemble(&code) {
                    println!("{instruction}");
                }
            }
        }
        [hex_code, rest @ ..] => execute_args("ton bytecode", hex_code, rest, false),
    }
}

/// Exécute le bytecode avec ses arguments facultatifs `[gas] [calldata]`. Un calldata qui
/// commence par 0x peut être donné seul, sans le gas.
fn execute_args(description: &str, hex_code: &str, args: &[&str], trace: bool) {
    let (gas, calldata) = match *args {
        [] => (None, ""),
        [calldata] if calldata.starts_with("0x") => (None, calldata),
        [gas] => (Some(gas), ""),
        [gas, calldata] => (Some(gas), calldata),
        _ => {
            eprintln!("trop d'arguments : <bytecode hex> [gas] [calldata hex]");
            return;
        }
    };
    let gas_limit = match gas {
        None => DEFAULT_GAS,
        Some(text) => match text.parse() {
            Ok(gas) => gas,
            Err(_) => {
                eprintln!("gas invalide : {text} (un calldata seul doit commencer par 0x)");
                return;
            }
        },
    };
    execute(description, hex_code, calldata, gas_limit, trace);
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

/// Décode un argument hexadécimal (`what` : bytecode ou calldata), ou affiche pourquoi
/// c'est impossible.
fn decode(what: &str, text: &str) -> Option<Vec<u8>> {
    let cleaned: String = text
        .trim_start_matches("0x")
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    hex::decode(cleaned)
        .inspect_err(|e| eprintln!("{what} hexadécimal invalide : {e}"))
        .ok()
}

/// Affiche une ligne par instruction : position, nom, gas restant, pile.
fn print_step(vm: &Vm) {
    let name = vm
        .current_opcode()
        .map_or_else(|| "-".to_string(), |op| op.to_string());
    let stack: Vec<String> = vm.stack().as_slice().iter().map(Word::to_string).collect();
    println!(
        "  {:04x}  {name:<12} gas {:<8} pile [{}]",
        vm.pc(),
        vm.gas_remaining().value(),
        stack.join(", ")
    );
}

fn execute(description: &str, hex_code: &str, calldata_hex: &str, gas_limit: u64, trace: bool) {
    let Some(code) = decode("bytecode", hex_code) else {
        return;
    };
    let Some(calldata) = decode("calldata", calldata_hex) else {
        return;
    };

    println!("▶ {description}");
    println!(
        "  bytecode : {}   (gas donné : {gas_limit})",
        hex::encode(&code)
    );
    if !calldata.is_empty() {
        println!("  calldata : {}", hex::encode(&calldata));
    }

    let mut vm = Vm::new(&code, Gas::new(gas_limit)).with_calldata(&calldata);
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
        let value = Word::from_be_slice(data);
        text.push_str(&format!("\n  en décimal : {value}"));
    }
    text
}
