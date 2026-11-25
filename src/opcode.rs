//! Opcodes de la mini-EVM et leur décodage depuis un octet.
//!
//! Les valeurs hexadécimales sont celles de la vraie EVM (voir evm.codes).

use std::fmt;

use crate::error::{Result, VmError};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Opcode {
    // 0x00 - 0x0f : arrêt et arithmétique
    Stop,
    Add,
    Mul,
    Sub,
    Div,
    Mod,
    Exp,
    // 0x10 - 0x1f : comparaison et bits
    Lt,
    Gt,
    Eq,
    IsZero,
    And,
    Or,
    Xor,
    Not,
    Shl,
    Shr,
    // 0x20 - 0x3f : hash et environnement
    Keccak256,
    Caller,
    // 0x50 - 0x5b : pile, mémoire, storage, contrôle
    Pop,
    MLoad,
    MStore,
    MStore8,
    SLoad,
    SStore,
    Jump,
    JumpI,
    Pc,
    MSize,
    Gas,
    JumpDest,
    /// PUSH0..PUSH32 : le nombre d'octets de donnée qui suivent (0..=32).
    Push(u8),
    /// DUP1..DUP16 : la profondeur de l'élément copié (1..=16).
    Dup(u8),
    /// SWAP1..SWAP16 : avec quel élément le haut est échangé (1..=16).
    Swap(u8),
    // 0xf0 - 0xff : fin d'exécution
    Return,
    Revert,
    Invalid,
}

impl Opcode {
    /// Décode un octet de bytecode. Seule source de `InvalidOpcode`.
    pub fn from_byte(byte: u8) -> Result<Opcode> {
        Ok(match byte {
            0x00 => Opcode::Stop,
            0x01 => Opcode::Add,
            0x02 => Opcode::Mul,
            0x03 => Opcode::Sub,
            0x04 => Opcode::Div,
            0x06 => Opcode::Mod,
            0x0a => Opcode::Exp,
            0x10 => Opcode::Lt,
            0x11 => Opcode::Gt,
            0x14 => Opcode::Eq,
            0x15 => Opcode::IsZero,
            0x16 => Opcode::And,
            0x17 => Opcode::Or,
            0x18 => Opcode::Xor,
            0x19 => Opcode::Not,
            0x1b => Opcode::Shl,
            0x1c => Opcode::Shr,
            0x20 => Opcode::Keccak256,
            0x33 => Opcode::Caller,
            0x50 => Opcode::Pop,
            0x51 => Opcode::MLoad,
            0x52 => Opcode::MStore,
            0x53 => Opcode::MStore8,
            0x54 => Opcode::SLoad,
            0x55 => Opcode::SStore,
            0x56 => Opcode::Jump,
            0x57 => Opcode::JumpI,
            0x58 => Opcode::Pc,
            0x59 => Opcode::MSize,
            0x5a => Opcode::Gas,
            0x5b => Opcode::JumpDest,
            0x5f..=0x7f => Opcode::Push(byte - 0x5f), // 0x5f = PUSH0, 0x60 = PUSH1, ...
            0x80..=0x8f => Opcode::Dup(byte - 0x7f),  // 0x80 = DUP1
            0x90..=0x9f => Opcode::Swap(byte - 0x8f), // 0x90 = SWAP1
            0xf3 => Opcode::Return,
            0xfd => Opcode::Revert,
            0xfe => Opcode::Invalid,
            other => return Err(VmError::InvalidOpcode(other)),
        })
    }

    /// L'octet qui encode cet opcode (inverse de `from_byte`).
    pub fn to_byte(self) -> u8 {
        match self {
            Opcode::Stop => 0x00,
            Opcode::Add => 0x01,
            Opcode::Mul => 0x02,
            Opcode::Sub => 0x03,
            Opcode::Div => 0x04,
            Opcode::Mod => 0x06,
            Opcode::Exp => 0x0a,
            Opcode::Lt => 0x10,
            Opcode::Gt => 0x11,
            Opcode::Eq => 0x14,
            Opcode::IsZero => 0x15,
            Opcode::And => 0x16,
            Opcode::Or => 0x17,
            Opcode::Xor => 0x18,
            Opcode::Not => 0x19,
            Opcode::Shl => 0x1b,
            Opcode::Shr => 0x1c,
            Opcode::Keccak256 => 0x20,
            Opcode::Caller => 0x33,
            Opcode::Pop => 0x50,
            Opcode::MLoad => 0x51,
            Opcode::MStore => 0x52,
            Opcode::MStore8 => 0x53,
            Opcode::SLoad => 0x54,
            Opcode::SStore => 0x55,
            Opcode::Jump => 0x56,
            Opcode::JumpI => 0x57,
            Opcode::Pc => 0x58,
            Opcode::MSize => 0x59,
            Opcode::Gas => 0x5a,
            Opcode::JumpDest => 0x5b,
            Opcode::Push(n) => 0x5f + n,
            Opcode::Dup(n) => 0x7f + n,
            Opcode::Swap(n) => 0x8f + n,
            Opcode::Return => 0xf3,
            Opcode::Revert => 0xfd,
            Opcode::Invalid => 0xfe,
        }
    }

    /// Nombre d'octets occupés dans le bytecode : l'opcode + ses données éventuelles.
    /// C'est ce dont le program counter avance après l'instruction.
    #[inline]
    pub fn size(self) -> usize {
        match self {
            Opcode::Push(n) => 1 + n as usize,
            _ => 1,
        }
    }
}

/// Nom lisible façon assembleur EVM : `ADD`, `PUSH1`, `DUP16`...
impl fmt::Display for Opcode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Opcode::Push(n) => return write!(f, "PUSH{n}"),
            Opcode::Dup(n) => return write!(f, "DUP{n}"),
            Opcode::Swap(n) => return write!(f, "SWAP{n}"),
            Opcode::Stop => "STOP",
            Opcode::Add => "ADD",
            Opcode::Mul => "MUL",
            Opcode::Sub => "SUB",
            Opcode::Div => "DIV",
            Opcode::Mod => "MOD",
            Opcode::Exp => "EXP",
            Opcode::Lt => "LT",
            Opcode::Gt => "GT",
            Opcode::Eq => "EQ",
            Opcode::IsZero => "ISZERO",
            Opcode::And => "AND",
            Opcode::Or => "OR",
            Opcode::Xor => "XOR",
            Opcode::Not => "NOT",
            Opcode::Shl => "SHL",
            Opcode::Shr => "SHR",
            Opcode::Keccak256 => "KECCAK256",
            Opcode::Caller => "CALLER",
            Opcode::Pop => "POP",
            Opcode::MLoad => "MLOAD",
            Opcode::MStore => "MSTORE",
            Opcode::MStore8 => "MSTORE8",
            Opcode::SLoad => "SLOAD",
            Opcode::SStore => "SSTORE",
            Opcode::Jump => "JUMP",
            Opcode::JumpI => "JUMPI",
            Opcode::Pc => "PC",
            Opcode::MSize => "MSIZE",
            Opcode::Gas => "GAS",
            Opcode::JumpDest => "JUMPDEST",
            Opcode::Return => "RETURN",
            Opcode::Revert => "REVERT",
            Opcode::Invalid => "INVALID",
        };
        f.write_str(name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_basic_opcodes() {
        assert_eq!(Opcode::from_byte(0x00).unwrap(), Opcode::Stop);
        assert_eq!(Opcode::from_byte(0x01).unwrap(), Opcode::Add);
        assert_eq!(Opcode::from_byte(0x52).unwrap(), Opcode::MStore);
        assert_eq!(Opcode::from_byte(0x5b).unwrap(), Opcode::JumpDest);
        assert_eq!(Opcode::from_byte(0xf3).unwrap(), Opcode::Return);
    }

    #[test]
    fn decodes_push_dup_swap_ranges() {
        assert_eq!(Opcode::from_byte(0x5f).unwrap(), Opcode::Push(0));
        assert_eq!(Opcode::from_byte(0x60).unwrap(), Opcode::Push(1));
        assert_eq!(Opcode::from_byte(0x7f).unwrap(), Opcode::Push(32));
        assert_eq!(Opcode::from_byte(0x80).unwrap(), Opcode::Dup(1));
        assert_eq!(Opcode::from_byte(0x8f).unwrap(), Opcode::Dup(16));
        assert_eq!(Opcode::from_byte(0x90).unwrap(), Opcode::Swap(1));
        assert_eq!(Opcode::from_byte(0x9f).unwrap(), Opcode::Swap(16));
    }

    #[test]
    fn unknown_byte_is_invalid_opcode() {
        assert_eq!(
            Opcode::from_byte(0x0c).unwrap_err(),
            VmError::InvalidOpcode(0x0c)
        );
    }

    #[test]
    fn every_byte_decodes_or_errors_and_round_trips() {
        for byte in 0..=u8::MAX {
            match Opcode::from_byte(byte) {
                Ok(op) => assert_eq!(op.to_byte(), byte, "aller-retour raté pour {op}"),
                Err(e) => assert_eq!(e, VmError::InvalidOpcode(byte)),
            }
        }
    }

    #[test]
    fn size_counts_push_data() {
        assert_eq!(Opcode::Add.size(), 1);
        assert_eq!(Opcode::Push(0).size(), 1);
        assert_eq!(Opcode::Push(1).size(), 2);
        assert_eq!(Opcode::Push(32).size(), 33);
    }

    #[test]
    fn display_uses_assembly_names() {
        assert_eq!(Opcode::Add.to_string(), "ADD");
        assert_eq!(Opcode::Push(1).to_string(), "PUSH1");
        assert_eq!(Opcode::Dup(16).to_string(), "DUP16");
    }
}
