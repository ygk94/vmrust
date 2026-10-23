//! Erreurs de la VM.

use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum VmError {
    #[error("invalid op code : {0:#04x}")]
    InvalidOpcode(u8), //cette erreur arrive quand la vm ne reconnait aucun opcode donc c'est lié au byte code, s'il ne lit pas bien le pb
    //Genre si on veut pousser 05 commee element de la pile on fait : 60 05
    //60 correspond au push vers une pile.
    #[error("invalid jump destination :{0}")]
    InvalidJumpsize(usize),
    #[error("stack underflow : needed {needed}, had {available}")]
    StackUnderflow { needed: usize, available: usize },
    #[error("stack overflow : limit of {limit} items exceed")]
    StackOverflow { limit: usize },
    #[error("out of gas : needed {needed}, remaining {remaining}")]
    OutOfGas { needed: u64, remaining: u64 },
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
}
