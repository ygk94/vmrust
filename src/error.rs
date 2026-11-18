//! Erreurs de la VM.

use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum VmError {
    /// Octet qui ne correspond à aucun opcode connu.
    // Boucle : 2. COMPRENDRE. Levée par Opcode::from_byte (et par l'opcode INVALID).
    #[error("invalid op code : {0:#04x}")]
    InvalidOpcode(u8),
    /// Saut vers une position qui n'est pas un JUMPDEST valide.
    // Boucle : 4. FAIRE, pendant JUMP / JUMPI. Levée par Vm::jump.
    #[error("invalid jump destination : {0}")]
    InvalidJump(usize),
    /// Pas assez d'éléments sur la pile pour l'opcode.
    // Boucle : 4. FAIRE, quand l'instruction prend ses arguments. Levée par Stack::pop / require / dup / swap.
    #[error("stack underflow : needed {needed}, had {available}")]
    StackUnderflow { needed: usize, available: usize },
    /// Plus de 1024 éléments sur la pile.
    // Boucle : 4. FAIRE, quand l'instruction pose son résultat. Levée par Stack::push / dup.
    #[error("stack overflow : limit of {limit} items exceeded")]
    StackOverflow { limit: usize },
    /// Le gas restant ne couvre pas le coût de l'instruction.
    // Boucle : 3. PAYER, au début de chaque tour, et en plus pendant les instructions mémoire.
    // Levée par Gas::charge.
    #[error("out of gas : needed {needed}, remaining {remaining}")]
    OutOfGas { needed: u64, remaining: u64 },
    /// Offset ou taille mémoire trop grand pour être adressé.
    // Boucle : 3. PAYER / 4. FAIRE, pendant MLOAD, MSTORE, RETURN... Levée par to_usize et
    // Memory::required_size.
    #[error("invalid memory offset : out of addressable range")]
    InvalidMemoryOffset,
}

pub type Result<T> = std::result::Result<T, VmError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_opcode_message_show_hex_byte() {
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
