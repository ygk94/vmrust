//! Types du domaine : mots de 256 bits, gas, adresses, compteur de programme.

use std::fmt;
use std::str::FromStr;

use ruint::aliases::U256;
use serde::{Deserialize, Serialize};

use crate::error::{Result, VmError};

/// Mot de 256 bits : l'unité de la pile, de la mémoire et du storage.
pub type Word = U256;

/// Quantité de gas.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Gas(u64);

impl Gas {
    pub fn new(amount: u64) -> Self {
        Gas(amount)
    }

    pub fn value(self) -> u64 {
        self.0
    }

    /// Facture `cost` sur le gas restant. Renvoie `OutOfGas` sans rien modifier
    /// si le gas restant ne suffit pas.
    pub fn charge(&mut self, cost: Gas) -> Result<()> {
        if cost.0 > self.0 {
            return Err(VmError::OutOfGas {
                needed: cost.0,
                remaining: self.0,
            });
        }
        self.0 -= cost.0;
        Ok(())
    }
}

/// Position de la prochaine instruction à lire dans le bytecode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ProgramCounter(usize);

impl ProgramCounter {
    pub fn get(self) -> usize {
        self.0
    }

    /// Avance de `n` octets : 1 pour un opcode normal, 1 + n pour un PUSHn.
    pub fn advance(&mut self, n: usize) {
        self.0 += n;
    }

    /// Place le pc sur `destination` (JUMP, JUMPI). La validité de la destination
    /// est vérifiée par l'interpréteur.
    pub fn jump_to(&mut self, destination: usize) {
        self.0 = destination;
    }
}

/// Adresse Ethereum (20 octets).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct Address([u8; 20]);

/// Sur la pile, une adresse est un mot aligné à droite : on garde les 20 derniers octets.
impl From<Word> for Address {
    fn from(word: Word) -> Self {
        let bytes: [u8; 32] = word.to_be_bytes();
        let mut address = [0u8; 20];
        address.copy_from_slice(&bytes[12..]);
        Address(address)
    }
}

/// L'inverse : complète par 12 octets de zéros à gauche (opcode CALLER).
impl From<Address> for Word {
    fn from(address: Address) -> Self {
        Word::from_be_slice(&address.0)
    }
}

/// Lit 40 caractères hexadécimaux, avec ou sans `0x`.
impl FromStr for Address {
    type Err = hex::FromHexError;

    fn from_str(text: &str) -> std::result::Result<Self, Self::Err> {
        let mut bytes = [0u8; 20];
        hex::decode_to_slice(text.strip_prefix("0x").unwrap_or(text), &mut bytes)?;
        Ok(Address(bytes))
    }
}

/// Affiche l'adresse au format `0x` + 40 caractères hexadécimaux.
impl fmt::Display for Address {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "0x{}", hex::encode(self.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn charge_subtracts_cost_from_remaining_gas() {
        let mut gas = Gas::new(10);
        gas.charge(Gas::new(3)).unwrap();
        assert_eq!(gas.value(), 7);
    }

    #[test]
    fn charge_fails_when_cost_exceeds_remaining() {
        let mut gas = Gas::new(1);
        let err = gas.charge(Gas::new(5)).unwrap_err();
        assert_eq!(
            err,
            VmError::OutOfGas {
                needed: 5,
                remaining: 1
            }
        );
        assert_eq!(gas.value(), 1);
    }

    #[test]
    fn program_counter_advances_and_jumps() {
        let mut pc = ProgramCounter::default();
        pc.advance(2);
        assert_eq!(pc.get(), 2);
        pc.jump_to(10);
        assert_eq!(pc.get(), 10);
    }

    #[test]
    fn address_parses_displays_and_converts_back_to_a_word() {
        let text = "0x000000000000000000000000000000000000abcd";
        let address: Address = text.parse().unwrap();
        assert_eq!(address.to_string(), text);
        let word: Word = address.into();
        assert_eq!(word, Word::from(0xabcdu64));
        assert!("0x12".parse::<Address>().is_err());
    }

    #[test]
    fn address_keeps_the_last_20_bytes_of_a_word() {
        let word = Word::from(0xabcdu64);
        let address = Address::from(word);
        assert_eq!(
            address.to_string(),
            "0x000000000000000000000000000000000000abcd"
        );
    }
}
