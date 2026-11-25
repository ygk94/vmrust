//! Coûts en gas des opcodes et calcul du coût d'agrandissement de la mémoire.
//!
//! Tous les prix de la VM sont ici : l'interpréteur ne contient aucun nombre magique.
//! Valeurs reprises de l'EVM (paliers du Yellow Paper).

use crate::opcode::Opcode;
use crate::types::{Gas, Word};

/// STOP, RETURN, REVERT, INVALID, et SSTORE (dont le coût est entièrement dynamique).
pub const ZERO: u64 = 0;
/// JUMPDEST.
pub const JUMPDEST: u64 = 1;
/// POP, PC, MSIZE, GAS, CALLER, PUSH0.
pub const BASE: u64 = 2;
/// ADD, SUB, comparaisons, bits, PUSH1-32, DUP, SWAP, MLOAD, MSTORE, MSTORE8.
pub const VERY_LOW: u64 = 3;
/// MUL, DIV, MOD.
pub const LOW: u64 = 5;
/// JUMP.
pub const MID: u64 = 8;
/// JUMPI, et partie fixe de EXP.
pub const HIGH: u64 = 10;
/// EXP, par octet significatif de l'exposant.
pub const EXP_BYTE: u64 = 50;
/// KECCAK256, partie fixe.
pub const KECCAK_BASE: u64 = 30;
/// KECCAK256, par mot de 32 octets hashé.
pub const KECCAK_WORD: u64 = 6;
/// SLOAD (simplifié : pas de distinction accès froid / chaud).
pub const SLOAD: u64 = 2_100;
/// SSTORE qui remplit une case vide.
pub const SSTORE_SET: u64 = 20_000;
/// Tout autre SSTORE (simplifié : pas de remboursement).
pub const SSTORE_RESET: u64 = 5_000;
/// Partie linéaire du coût mémoire, par mot de 32 octets.
pub const MEMORY_WORD: u64 = 3;
/// Diviseur de la partie quadratique du coût mémoire.
pub const MEMORY_QUADRATIC_DIVISOR: u64 = 512;

/// Coût fixe de l'opcode, payé avant son exécution.
#[inline]
pub fn static_cost(op: Opcode) -> Gas {
    Gas::new(match op {
        Opcode::Stop | Opcode::Return | Opcode::Revert | Opcode::SStore | Opcode::Invalid => ZERO,
        Opcode::JumpDest => JUMPDEST,
        Opcode::Pop
        | Opcode::Pc
        | Opcode::MSize
        | Opcode::Gas
        | Opcode::Caller
        | Opcode::Push(0) => BASE,
        Opcode::Add
        | Opcode::Sub
        | Opcode::Lt
        | Opcode::Gt
        | Opcode::Eq
        | Opcode::IsZero
        | Opcode::And
        | Opcode::Or
        | Opcode::Xor
        | Opcode::Not
        | Opcode::Shl
        | Opcode::Shr
        | Opcode::MLoad
        | Opcode::MStore
        | Opcode::MStore8
        | Opcode::Push(_)
        | Opcode::Dup(_)
        | Opcode::Swap(_) => VERY_LOW,
        Opcode::Mul | Opcode::Div | Opcode::Mod => LOW,
        Opcode::Jump => MID,
        Opcode::JumpI | Opcode::Exp => HIGH,
        Opcode::Keccak256 => KECCAK_BASE,
        Opcode::SLoad => SLOAD,
    })
}

/// Coût dynamique de EXP : dépend de la taille de l'exposant.
pub fn exp_cost(exponent: Word) -> Gas {
    Gas::new(EXP_BYTE * exponent.byte_len() as u64)
}

/// Coût dynamique de KECCAK256 : par mot hashé, arrondi au supérieur.
pub fn keccak_cost(len: usize) -> Gas {
    Gas::new(KECCAK_WORD.saturating_mul((len as u64).div_ceil(32)))
}

/// Coût de SSTORE selon la valeur actuelle de la case et la nouvelle.
pub fn sstore_cost(current: Word, new: Word) -> Gas {
    Gas::new(if current.is_zero() && !new.is_zero() {
        SSTORE_SET
    } else {
        SSTORE_RESET
    })
}

/// Coût TOTAL d'une mémoire de `words` mots : `3 * words + words² / 512`.
///
/// Linéaire au début, quadratique ensuite : c'est ce qui rend les grosses mémoires
/// hors de prix. Saturant pour ne jamais déborder sur des tailles absurdes.
pub fn memory_cost(words: u64) -> u64 {
    MEMORY_WORD
        .saturating_mul(words)
        .saturating_add(words.saturating_mul(words) / MEMORY_QUADRATIC_DIVISOR)
}

/// Ce qu'il faut payer pour passer de `old_words` à `new_words`. 0 si pas d'agrandissement.
pub fn memory_expansion_cost(old_words: u64, new_words: u64) -> Gas {
    if new_words <= old_words {
        return Gas::new(0);
    }
    Gas::new(memory_cost(new_words).saturating_sub(memory_cost(old_words)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn static_costs_follow_evm_tiers() {
        assert_eq!(static_cost(Opcode::Stop).value(), 0);
        assert_eq!(static_cost(Opcode::JumpDest).value(), 1);
        assert_eq!(static_cost(Opcode::Push(0)).value(), 2);
        assert_eq!(static_cost(Opcode::Push(1)).value(), 3);
        assert_eq!(static_cost(Opcode::Add).value(), 3);
        assert_eq!(static_cost(Opcode::Mul).value(), 5);
        assert_eq!(static_cost(Opcode::Jump).value(), 8);
        assert_eq!(static_cost(Opcode::JumpI).value(), 10);
    }

    #[test]
    fn memory_cost_known_values() {
        assert_eq!(memory_cost(0), 0);
        assert_eq!(memory_cost(1), 3);
        assert_eq!(memory_cost(32), 98); // 96 + 1024/512
        assert_eq!(memory_cost(32_768), 98_304 + 2_097_152); // 1 Mo
    }

    #[test]
    fn memory_cost_saturates_instead_of_overflowing() {
        assert_eq!(memory_cost(u64::MAX), u64::MAX);
    }

    #[test]
    fn expansion_pays_only_the_difference() {
        assert_eq!(memory_expansion_cost(0, 1).value(), 3);
        assert_eq!(memory_expansion_cost(2, 3).value(), 3);
        assert_eq!(memory_expansion_cost(0, 32).value(), 98);
    }

    #[test]
    fn dynamic_costs() {
        assert_eq!(exp_cost(Word::ZERO).value(), 0);
        assert_eq!(exp_cost(Word::from(255u64)).value(), 50);
        assert_eq!(exp_cost(Word::from(256u64)).value(), 100);
        assert_eq!(exp_cost(Word::MAX).value(), 1_600);
        assert_eq!(keccak_cost(0).value(), 0);
        assert_eq!(keccak_cost(33).value(), 12);
        let (zero, one) = (Word::ZERO, Word::from(1u64));
        assert_eq!(sstore_cost(zero, one).value(), 20_000);
        assert_eq!(sstore_cost(one, zero).value(), 5_000);
        assert_eq!(sstore_cost(zero, zero).value(), 5_000);
    }

    #[test]
    fn no_expansion_is_free() {
        assert_eq!(memory_expansion_cost(5, 5).value(), 0);
        assert_eq!(memory_expansion_cost(5, 2).value(), 0);
    }
}
