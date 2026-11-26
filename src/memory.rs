//! Mémoire volatile de la VM, adressée en octets et agrandie par mots de 32 octets.
//!
//! Règles :
//! - tout octet jamais écrit vaut 0 ;
//! - la taille est toujours un multiple de 32 ;
//! - accéder au-delà de la taille l'agrandit (l'interpréteur facture le gas AVANT,
//!   grâce à `Memory::required_size`).

use crate::error::{Result, VmError};
use crate::types::Word;

/// Taille d'un mot EVM en octets.
pub const WORD_SIZE: usize = 32;

/// Plafond de sécurité : au-delà, on refuse plutôt que d'allouer.
/// 32 Mio coûteraient déjà plus de 2 milliards de gas.
pub const MEMORY_LIMIT: usize = 32 * 1024 * 1024;

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Memory {
    data: Vec<u8>,
}

/// Arrondit au multiple de 32 supérieur : 1 -> 32, 32 -> 32, 33 -> 64.
fn round_up_to_word(n: usize) -> usize {
    n.div_ceil(WORD_SIZE) * WORD_SIZE
}

impl Memory {
    pub fn new() -> Self {
        Self::default()
    }

    /// Taille actuelle en octets (multiple de 32).
    pub fn size(&self) -> usize {
        self.data.len()
    }

    /// Taille actuelle en mots de 32 octets.
    pub fn size_in_words(&self) -> usize {
        self.data.len() / WORD_SIZE
    }

    /// Taille (en octets, arrondie au mot) que doit avoir la mémoire pour couvrir
    /// `[offset, offset + len)`. Renvoie 0 si `len == 0` : lire 0 octet n'agrandit rien,
    /// même à un offset énorme.
    ///
    /// Erreur si la zone déborde ou dépasse `MEMORY_LIMIT`. C'est l'interpréteur qui
    /// appelle ça pour facturer le gas avant tout agrandissement.
    pub fn required_size(offset: usize, len: usize) -> Result<usize> {
        if len == 0 {
            return Ok(0);
        }
        let end = offset
            .checked_add(len)
            .filter(|&end| end <= MEMORY_LIMIT)
            .ok_or(VmError::InvalidMemoryOffset)?;
        Ok(round_up_to_word(end))
    }

    /// Agrandit pour couvrir `[offset, offset + len)`, arrondi au mot supérieur.
    /// Ne rétrécit jamais. Sans effet si `len == 0`.
    ///
    /// Panique si la zone dépasse `MEMORY_LIMIT` : l'appelant doit l'avoir validée
    /// avec `required_size` avant.
    pub fn expand(&mut self, offset: usize, len: usize) {
        let needed =
            Self::required_size(offset, len).expect("zone mémoire non validée par required_size");
        if needed > self.data.len() {
            self.data.resize(needed, 0);
        }
    }

    /// Lit 32 octets à partir de `offset`, en big-endian (MLOAD).
    pub fn load_word(&mut self, offset: usize) -> Word {
        self.expand(offset, WORD_SIZE);
        Word::from_be_slice(&self.data[offset..offset + WORD_SIZE])
    }

    /// Écrit 32 octets à partir de `offset`, en big-endian (MSTORE).
    pub fn store_word(&mut self, offset: usize, value: Word) {
        self.expand(offset, WORD_SIZE);
        self.data[offset..offset + WORD_SIZE].copy_from_slice(&value.to_be_bytes::<WORD_SIZE>());
    }

    /// Écrit un seul octet (MSTORE8).
    pub fn store_byte(&mut self, offset: usize, value: u8) {
        self.expand(offset, 1);
        self.data[offset] = value;
    }

    /// Écrit `data` à partir de `offset` (CALLDATACOPY). Sans effet si `data` est vide.
    pub fn write(&mut self, offset: usize, data: &[u8]) {
        if data.is_empty() {
            return;
        }
        self.expand(offset, data.len());
        self.data[offset..offset + data.len()].copy_from_slice(data);
    }

    /// Copie de `[offset, offset + len)` (RETURN, REVERT). Vide si `len == 0`.
    pub fn read(&mut self, offset: usize, len: usize) -> Vec<u8> {
        if len == 0 {
            return Vec::new();
        }
        self.expand(offset, len);
        self.data[offset..offset + len].to_vec()
    }

    /// Vue sur tout le contenu.
    pub fn as_slice(&self) -> &[u8] {
        &self.data
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starts_empty() {
        let m = Memory::new();
        assert_eq!(m.size(), 0);
        assert_eq!(m.size_in_words(), 0);
    }

    #[test]
    fn store_then_load_word() {
        let mut m = Memory::new();
        m.store_word(0, Word::from(0xdead_beefu64));
        assert_eq!(m.load_word(0), Word::from(0xdead_beefu64));
    }

    #[test]
    fn store_word_is_big_endian() {
        let mut m = Memory::new();
        m.store_word(0, Word::from(5u64));
        assert_eq!(m.as_slice()[..31], [0u8; 31]);
        assert_eq!(m.as_slice()[31], 5);
    }

    #[test]
    fn unwritten_memory_reads_as_zero() {
        let mut m = Memory::new();
        assert_eq!(m.load_word(64), Word::ZERO);
        assert_eq!(m.size(), 96);
    }

    #[test]
    fn expansion_rounds_up_to_32_bytes() {
        let mut m = Memory::new();
        m.store_byte(0, 1);
        assert_eq!(m.size(), 32);
        m.store_byte(32, 1);
        assert_eq!(m.size(), 64);
    }

    #[test]
    fn unaligned_word_spans_two_words() {
        let mut m = Memory::new();
        m.store_word(1, Word::MAX);
        assert_eq!(m.size(), 64);
        assert_eq!(m.as_slice()[0], 0);
        assert_eq!(m.load_word(1), Word::MAX);
    }

    #[test]
    fn zero_length_never_expands() {
        let mut m = Memory::new();
        assert!(m.read(1_000_000, 0).is_empty());
        m.expand(usize::MAX, 0);
        assert_eq!(m.size(), 0);
    }

    #[test]
    fn write_copies_bytes_and_expands() {
        let mut m = Memory::new();
        m.write(1, &[0xaa, 0xbb]);
        assert_eq!(m.size(), 32);
        assert_eq!(m.read(0, 4), vec![0x00, 0xaa, 0xbb, 0x00]);
        m.write(usize::MAX, &[]);
        assert_eq!(m.size(), 32);
    }

    #[test]
    fn read_returns_a_slice_copy() {
        let mut m = Memory::new();
        m.store_byte(2, 0xaa);
        assert_eq!(m.read(1, 3), vec![0x00, 0xaa, 0x00]);
    }

    #[test]
    fn required_size_rounds_and_ignores_empty_ranges() {
        assert_eq!(Memory::required_size(0, 0).unwrap(), 0);
        assert_eq!(Memory::required_size(usize::MAX, 0).unwrap(), 0);
        assert_eq!(Memory::required_size(0, 1).unwrap(), 32);
        assert_eq!(Memory::required_size(31, 2).unwrap(), 64);
    }

    #[test]
    fn required_size_rejects_overflow_and_huge_ranges() {
        assert_eq!(
            Memory::required_size(usize::MAX, 32).unwrap_err(),
            VmError::InvalidMemoryOffset
        );
        assert_eq!(
            Memory::required_size(MEMORY_LIMIT, 1).unwrap_err(),
            VmError::InvalidMemoryOffset
        );
    }
}
