//! Désassembleur : transforme du bytecode en instructions lisibles.

use std::fmt;

use crate::opcode::Opcode;

/// Une instruction décodée et sa position dans le bytecode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Instruction<'a> {
    pub offset: usize,
    pub byte: u8,
    /// `None` si l'octet n'est pas un opcode connu.
    pub opcode: Option<Opcode>,
    /// Données du PUSH (plus courtes que prévu si le code est tronqué).
    pub data: &'a [u8],
}

impl fmt::Display for Instruction<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:04x}  ", self.offset)?;
        match self.opcode {
            Some(op) => write!(f, "{op}")?,
            None => write!(f, "UNKNOWN({:#04x})", self.byte)?,
        }
        if !self.data.is_empty() {
            write!(f, " 0x{}", hex::encode(self.data))?;
        }
        Ok(())
    }
}

pub fn disassemble(code: &[u8]) -> Vec<Instruction<'_>> {
    let mut instructions = Vec::new();
    let mut offset = 0;
    while offset < code.len() {
        let byte = code[offset];
        let opcode = Opcode::from_byte(byte).ok();
        let end = (offset + opcode.map_or(1, Opcode::size)).min(code.len());
        instructions.push(Instruction {
            offset,
            byte,
            opcode,
            data: &code[offset + 1..end],
        });
        offset = end;
    }
    instructions
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(code: &[u8]) -> Vec<String> {
        disassemble(code).iter().map(|i| i.to_string()).collect()
    }

    #[test]
    fn shows_offsets_names_and_push_data() {
        assert_eq!(
            lines(&[0x61, 0x01, 0x02, 0x5f, 0x01]),
            ["0000  PUSH2 0x0102", "0003  PUSH0", "0004  ADD"]
        );
    }

    #[test]
    fn handles_unknown_bytes_and_truncated_push() {
        assert_eq!(
            lines(&[0x0c, 0x61, 0xaa]),
            ["0000  UNKNOWN(0x0c)", "0001  PUSH2 0xaa"]
        );
        assert_eq!(lines(&[0x7f]), ["0000  PUSH32"]);
    }
}
