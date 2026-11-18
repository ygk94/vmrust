//! Erreurs de la VM.

use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum VmError {
    /// Octet qui ne correspond à aucun opcode connu.
    #[error("invalid op code : {0:#04x}")]
    InvalidOpcode(u8),
    /// Saut vers une position qui n'est pas un JUMPDEST valide.
    #[error("invalid jump destination : {0}")]
    InvalidJump(usize),
    /// Pas assez d'éléments sur la pile pour l'opcode.
    #[error("stack underflow : needed {needed}, had {available}")]
    StackUnderflow { needed: usize, available: usize },
    /// Plus de 1024 éléments sur la pile.
    #[error("stack overflow : limit of {limit} items exceeded")]
    StackOverflow { limit: usize },
    /// Le gas restant ne couvre pas le coût de l'instruction.
    #[error("out of gas : needed {needed}, remaining {remaining}")]
    OutOfGas { needed: u64, remaining: u64 },
    /// Offset ou taille mémoire trop grand pour être adressé.
    #[error("invalid memory offset : out of addressable range")]
    InvalidMemoryOffset,
}

pub type Result<T> = std::result::Result<T, VmError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_opcode_message_shows_hex_byte() {
        assert_eq!(
            VmError::InvalidOpcode(0x0c).to_string(),
            "invalid op code : 0x0c"
        );
    }

    #[test]
    fn out_of_gas_message_shows_both_amounts() {
        let err = VmError::OutOfGas {
            needed: 3,
            remaining: 1,
        };
        assert_eq!(err.to_string(), "out of gas : needed 3, remaining 1");
    }

    #[test]
    fn invalid_jump_message_shows_destination() {
        assert_eq!(
            VmError::InvalidJump(7).to_string(),
            "invalid jump destination : 7"
        );
    }

    #[test]
    fn stack_overflow_message_shows_limit() {
        assert_eq!(
            VmError::StackOverflow { limit: 1024 }.to_string(),
            "stack overflow : limit of 1024 items exceeded"
        );
    }
}
