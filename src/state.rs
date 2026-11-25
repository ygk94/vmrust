//! État du monde : code des contrats et storage, derrière le trait `StateBackend`.
//!
//! L'interpréteur n'écrit jamais directement ici pendant une exécution : il garde ses
//! écritures dans un journal et ne les applique qu'en cas de succès.

use std::collections::HashMap;

use crate::types::{Address, Word};

/// Ce qui survit entre deux exécutions, sans dire comment c'est stocké.
pub trait StateBackend {
    /// Code du contrat à cette adresse (vide si aucun).
    fn code(&self, address: &Address) -> Vec<u8>;

    /// Installe le code d'un contrat.
    fn set_code(&mut self, address: Address, code: Vec<u8>);

    /// Lit une case de storage. Une case jamais écrite vaut 0.
    fn sload(&self, address: &Address, key: Word) -> Word;

    /// Écrit une case de storage.
    fn sstore(&mut self, address: &Address, key: Word, value: Word);
}

/// Permet de prêter un état à la VM sans le lui donner.
impl<S: StateBackend> StateBackend for &mut S {
    fn code(&self, address: &Address) -> Vec<u8> {
        (**self).code(address)
    }

    fn set_code(&mut self, address: Address, code: Vec<u8>) {
        (**self).set_code(address, code);
    }

    fn sload(&self, address: &Address, key: Word) -> Word {
        (**self).sload(address, key)
    }

    fn sstore(&mut self, address: &Address, key: Word, value: Word) {
        (**self).sstore(address, key, value);
    }
}

/// État gardé en RAM.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct InMemoryState {
    codes: HashMap<Address, Vec<u8>>,
    storage: HashMap<(Address, Word), Word>,
}

impl StateBackend for InMemoryState {
    fn code(&self, address: &Address) -> Vec<u8> {
        self.codes.get(address).cloned().unwrap_or_default()
    }

    fn set_code(&mut self, address: Address, code: Vec<u8>) {
        self.codes.insert(address, code);
    }

    fn sload(&self, address: &Address, key: Word) -> Word {
        self.storage
            .get(&(*address, key))
            .copied()
            .unwrap_or(Word::ZERO)
    }

    fn sstore(&mut self, address: &Address, key: Word, value: Word) {
        if value.is_zero() {
            // 0 = case vide : on libère la place.
            self.storage.remove(&(*address, key));
        } else {
            self.storage.insert((*address, key), value);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn w(n: u64) -> Word {
        Word::from(n)
    }

    #[test]
    fn unwritten_slot_is_zero_and_unknown_code_is_empty() {
        let state = InMemoryState::default();
        assert_eq!(state.sload(&Address::default(), w(1)), w(0));
        assert!(state.code(&Address::default()).is_empty());
    }

    #[test]
    fn storage_is_separated_by_address() {
        let mut state = InMemoryState::default();
        let (a, b) = (Address::from(w(1)), Address::from(w(2)));
        state.sstore(&a, w(7), w(42));
        assert_eq!(state.sload(&a, w(7)), w(42));
        assert_eq!(state.sload(&b, w(7)), w(0));
    }

    #[test]
    fn writing_zero_frees_the_slot() {
        let mut state = InMemoryState::default();
        state.sstore(&Address::default(), w(1), w(5));
        state.sstore(&Address::default(), w(1), w(0));
        assert_eq!(state, InMemoryState::default());
    }

    #[test]
    fn set_code_then_read_it() {
        let mut state = InMemoryState::default();
        state.set_code(Address::default(), vec![1, 2]);
        assert_eq!(state.code(&Address::default()), vec![1, 2]);
    }
}
