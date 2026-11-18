//! Boucle d'interprétation : lecture, décodage et exécution des opcodes.
//!
//! `Vm::run` répète `step` jusqu'à STOP, RETURN, REVERT ou une erreur.
//! Une erreur (`Err(VmError)`) est un arrêt exceptionnel : tout le gas est perdu.
//! REVERT n'est PAS une erreur : c'est un résultat normal (`ExecutionOutcome::Revert`).

use crate::error::{Result, VmError};
use crate::gas;
use crate::memory::{Memory, WORD_SIZE};
use crate::opcode::Opcode;
use crate::stack::Stack;
use crate::types::{Gas, ProgramCounter, Word};

/// Résultat d'une exécution qui s'est terminée proprement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExecutionOutcome {
    /// STOP, RETURN, ou fin du code atteinte.
    Success { return_data: Vec<u8>, gas_used: u64 },
    /// REVERT : annulation volontaire, avec des données éventuelles.
    Revert { return_data: Vec<u8>, gas_used: u64 },
}

/// Ce que demande une instruction à la boucle principale.
enum Control {
    Continue,
    Stop,
    Return(Vec<u8>),
    Revert(Vec<u8>),
}

pub struct Vm<'a> {
    code: &'a [u8],
    pc: ProgramCounter,
    stack: Stack,
    memory: Memory,
    /// Gas restant.
    gas: Gas,
    gas_limit: Gas,
    /// `jumpdests[i] == true` si `code[i]` est un JUMPDEST qui n'est pas une donnée de PUSH.
    jumpdests: Vec<bool>,
}

impl<'a> Vm<'a> {
    pub fn new(code: &'a [u8], gas_limit: Gas) -> Self {
        Vm {
            code,
            pc: ProgramCounter::default(),
            stack: Stack::new(),
            memory: Memory::new(),
            gas: gas_limit,
            gas_limit,
            jumpdests: analyze_jumpdests(code),
        }
    }

    /// Exécute jusqu'au bout.
    pub fn run(&mut self) -> Result<ExecutionOutcome> {
        loop {
            match self.step()? {
                Control::Continue => {}
                Control::Stop => return Ok(self.success(Vec::new())),
                Control::Return(data) => return Ok(self.success(data)),
                Control::Revert(data) => return Ok(self.revert(data)),
            }
        }
    }

    pub fn stack(&self) -> &Stack {
        &self.stack
    }

    pub fn memory(&self) -> &Memory {
        &self.memory
    }

    pub fn gas_remaining(&self) -> Gas {
        self.gas
    }

    fn gas_used(&self) -> u64 {
        self.gas_limit.value() - self.gas.value()
    }

    fn success(&self, return_data: Vec<u8>) -> ExecutionOutcome {
        ExecutionOutcome::Success {
            return_data,
            gas_used: self.gas_used(),
        }
    }

    fn revert(&self, return_data: Vec<u8>) -> ExecutionOutcome {
        ExecutionOutcome::Revert {
            return_data,
            gas_used: self.gas_used(),
        }
    }

    /// Exécute UNE instruction : fetch, decode, facturation du gas, execute.
    fn step(&mut self) -> Result<Control> {
        // Fin du code = STOP implicite.
        let Some(&byte) = self.code.get(self.pc.get()) else {
            return Ok(Control::Stop);
        };
        let op = Opcode::from_byte(byte)?;
        self.gas.charge(gas::static_cost(op))?;

        match op {
            Opcode::Stop => return Ok(Control::Stop),

            Opcode::Add => self.binary_op(|a, b| a.wrapping_add(b))?,
            Opcode::Mul => self.binary_op(|a, b| a.wrapping_mul(b))?,
            Opcode::Sub => self.binary_op(|a, b| a.wrapping_sub(b))?,
            // Division et modulo par zéro valent 0 dans l'EVM (pas d'erreur).
            Opcode::Div => self.binary_op(|a, b| a.checked_div(b).unwrap_or(Word::ZERO))?,
            Opcode::Mod => self.binary_op(|a, b| a.checked_rem(b).unwrap_or(Word::ZERO))?,

            Opcode::Lt => self.binary_op(|a, b| bool_word(a < b))?,
            Opcode::Gt => self.binary_op(|a, b| bool_word(a > b))?,
            Opcode::Eq => self.binary_op(|a, b| bool_word(a == b))?,
            Opcode::IsZero => self.unary_op(|a| bool_word(a.is_zero()))?,
            Opcode::And => self.binary_op(|a, b| a & b)?,
            Opcode::Or => self.binary_op(|a, b| a | b)?,
            Opcode::Xor => self.binary_op(|a, b| a ^ b)?,
            Opcode::Not => self.unary_op(|a| !a)?,

            Opcode::Pop => {
                self.stack.pop()?;
            }
            Opcode::Push(n) => {
                let value = self.read_push_data(n as usize);
                self.stack.push(value)?;
            }
            Opcode::Dup(n) => self.stack.dup(n as usize)?,
            Opcode::Swap(n) => self.stack.swap(n as usize)?,

            Opcode::Pc => self.stack.push(Word::from(self.pc.get()))?,
            // GAS empile le gas restant APRÈS avoir payé le GAS lui-même.
            Opcode::Gas => self.stack.push(Word::from(self.gas.value()))?,

            Opcode::MLoad => {
                let offset = to_usize(self.stack.pop()?)?;
                self.charge_memory(offset, 32)?;
                let value = self.memory.load_word(offset);
                self.stack.push(value)?;
            }
            Opcode::MStore => {
                self.stack.require(2)?;
                let offset = to_usize(self.stack.pop()?)?;
                let value = self.stack.pop()?;
                self.charge_memory(offset, 32)?;
                self.memory.store_word(offset, value);
            }
            Opcode::MStore8 => {
                self.stack.require(2)?;
                let offset = to_usize(self.stack.pop()?)?;
                let value = self.stack.pop()?;
                self.charge_memory(offset, 1)?;
                self.memory.store_byte(offset, value.byte(0)); // octet de poids faible
            }
            Opcode::MSize => self.stack.push(Word::from(self.memory.size()))?,

            Opcode::Return | Opcode::Revert => {
                self.stack.require(2)?;
                let offset = self.stack.pop()?;
                let size = to_usize(self.stack.pop()?)?;
                // Taille 0 : rien à lire, l'offset est ignoré (même s'il est énorme).
                let data = if size == 0 {
                    Vec::new()
                } else {
                    let offset = to_usize(offset)?;
                    self.charge_memory(offset, size)?;
                    self.memory.read(offset, size)
                };
                return Ok(if op == Opcode::Return {
                    Control::Return(data)
                } else {
                    Control::Revert(data)
                });
            }

            Opcode::Jump => {
                let destination = self.stack.pop()?;
                self.jump(destination)?;
                return Ok(Control::Continue); // pc déjà placé : pas de advance()
            }
            Opcode::JumpI => {
                self.stack.require(2)?;
                let destination = self.stack.pop()?;
                let condition = self.stack.pop()?;
                if !condition.is_zero() {
                    self.jump(destination)?;
                    return Ok(Control::Continue);
                }
            }
            Opcode::JumpDest => {}

            Opcode::Invalid => return Err(VmError::InvalidOpcode(byte)),
        }

        self.pc.advance(op.size());
        Ok(Control::Continue)
    }

    /// Place le pc sur `destination` si c'est un JUMPDEST valide, sinon `InvalidJump`.
    fn jump(&mut self, destination: Word) -> Result<()> {
        // Une destination qui ne tient pas dans un usize est forcément hors du code.
        let destination = usize::try_from(destination).unwrap_or(usize::MAX);
        if self.jumpdests.get(destination) != Some(&true) {
            return Err(VmError::InvalidJump(destination));
        }
        self.pc.jump_to(destination);
        Ok(())
    }

    /// Facture l'agrandissement mémoire nécessaire pour couvrir `[offset, offset + len)`.
    /// Appelé AVANT de toucher la mémoire : si le gas manque, rien n'est alloué.
    fn charge_memory(&mut self, offset: usize, len: usize) -> Result<()> {
        let new_size = Memory::required_size(offset, len)?;
        if new_size > self.memory.size() {
            let old_words = self.memory.size_in_words() as u64;
            let new_words = (new_size / WORD_SIZE) as u64;
            self.gas
                .charge(gas::memory_expansion_cost(old_words, new_words))?;
        }
        Ok(())
    }

    /// Dépile `a` (le haut) puis `b`, empile `f(a, b)`.
    #[inline]
    fn binary_op(&mut self, f: impl FnOnce(Word, Word) -> Word) -> Result<()> {
        self.stack.require(2)?;
        let a = self.stack.pop()?;
        let b = self.stack.pop()?;
        self.stack.push(f(a, b))
    }

    /// Dépile `a`, empile `f(a)`.
    #[inline]
    fn unary_op(&mut self, f: impl FnOnce(Word) -> Word) -> Result<()> {
        let a = self.stack.pop()?;
        self.stack.push(f(a))
    }

    /// Lit les `n` octets qui suivent l'opcode PUSH courant, en big-endian.
    /// Si le code s'arrête avant, les octets manquants valent 0 (à droite).
    ///
    /// PUSH1 à PUSH8 sont assemblés dans un `u64` : un `copy_from_slice` de longueur
    /// variable appelait `memcpy` à chaque PUSH (~9 % du temps au flamegraph).
    #[inline]
    fn read_push_data(&self, n: usize) -> Word {
        let start = (self.pc.get() + 1).min(self.code.len());
        let available = &self.code[start..self.code.len().min(start + n)];

        if n <= 8 {
            let mut value = 0u64;
            for &byte in available {
                value = (value << 8) | byte as u64;
            }
            // Octets manquants en fin de code : des zéros à droite.
            // (n - len) <= 8, et le cas 8 n'arrive que si value == 0.
            let missing = n - available.len();
            value = value.checked_shl(8 * missing as u32).unwrap_or(0);
            return Word::from(value);
        }

        let mut bytes = [0u8; 32];
        bytes[32 - n..32 - n + available.len()].copy_from_slice(available);
        Word::from_be_bytes(bytes)
    }
}

/// Repère les JUMPDEST valides en un seul passage sur le code.
///
/// Un octet 0x5b situé dans les données d'un PUSH n'est PAS une destination valide :
/// sinon on pourrait sauter au milieu d'une donnée et l'exécuter comme du code.
fn analyze_jumpdests(code: &[u8]) -> Vec<bool> {
    let mut valid = vec![false; code.len()];
    let mut i = 0;
    while i < code.len() {
        let byte = code[i];
        if byte == Opcode::JumpDest.to_byte() {
            valid[i] = true;
        } else if (0x60..=0x7f).contains(&byte) {
            i += (byte - 0x5f) as usize; // saute les n octets de donnée du PUSHn
        }
        i += 1;
    }
    valid
}

/// Convertit un mot venant de la pile en offset/taille utilisable.
/// Jamais de `as` : un mot trop grand est une erreur, pas une troncature silencieuse.
fn to_usize(word: Word) -> Result<usize> {
    usize::try_from(word).map_err(|_| VmError::InvalidMemoryOffset)
}

/// Les booléens EVM sont des mots : 1 pour vrai, 0 pour faux.
#[inline]
fn bool_word(b: bool) -> Word {
    Word::from(u8::from(b))
}

#[cfg(test)]
mod tests {
    use super::*;

    const GAS: u64 = 1_000_000;

    fn run(code: &[u8]) -> Result<ExecutionOutcome> {
        Vm::new(code, Gas::new(GAS)).run()
    }

    /// Exécute et renvoie la pile finale (du bas vers le haut).
    fn final_stack(code: &[u8]) -> Vec<Word> {
        let mut vm = Vm::new(code, Gas::new(GAS));
        vm.run().unwrap();
        vm.stack().as_slice().to_vec()
    }

    fn w(n: u64) -> Word {
        Word::from(n)
    }

    #[test]
    fn empty_code_succeeds_without_gas() {
        assert_eq!(
            run(&[]).unwrap(),
            ExecutionOutcome::Success {
                return_data: vec![],
                gas_used: 0
            }
        );
    }

    #[test]
    fn adds_two_numbers() {
        // PUSH1 2, PUSH1 3, ADD
        assert_eq!(final_stack(&[0x60, 0x02, 0x60, 0x03, 0x01]), vec![w(5)]);
    }

    #[test]
    fn sub_is_top_minus_second() {
        // PUSH1 3, PUSH1 10, SUB  ->  10 - 3
        assert_eq!(final_stack(&[0x60, 0x03, 0x60, 0x0a, 0x03]), vec![w(7)]);
    }

    #[test]
    fn computes_2_plus_3_times_4_with_exact_gas() {
        // PUSH1 2, PUSH1 3, ADD, PUSH1 4, MUL, STOP  -> 3+3+3+3+5 = 17 gas
        let code = [0x60, 0x02, 0x60, 0x03, 0x01, 0x60, 0x04, 0x02, 0x00];
        let mut vm = Vm::new(&code, Gas::new(GAS));
        assert_eq!(
            vm.run().unwrap(),
            ExecutionOutcome::Success {
                return_data: vec![],
                gas_used: 17
            }
        );
        assert_eq!(vm.stack().as_slice(), &[w(20)]);
    }

    #[test]
    fn arithmetic_wraps_modulo_2_256() {
        // PUSH1 1, PUSH32 MAX, ADD  -> 0
        let mut code = vec![0x60, 0x01, 0x7f];
        code.extend([0xff; 32]);
        code.push(0x01);
        assert_eq!(final_stack(&code), vec![Word::ZERO]);
        // PUSH1 1, PUSH0, SUB  ->  0 - 1 = MAX
        assert_eq!(final_stack(&[0x60, 0x01, 0x5f, 0x03]), vec![Word::MAX]);
    }

    #[test]
    fn division_and_modulo_by_zero_give_zero() {
        // PUSH0, PUSH1 7, DIV  ->  7 / 0
        assert_eq!(final_stack(&[0x5f, 0x60, 0x07, 0x04]), vec![Word::ZERO]);
        assert_eq!(final_stack(&[0x5f, 0x60, 0x07, 0x06]), vec![Word::ZERO]);
        // PUSH1 3, PUSH1 7, DIV / MOD  ->  7 / 3 = 2, 7 % 3 = 1
        assert_eq!(final_stack(&[0x60, 0x03, 0x60, 0x07, 0x04]), vec![w(2)]);
        assert_eq!(final_stack(&[0x60, 0x03, 0x60, 0x07, 0x06]), vec![w(1)]);
    }

    #[test]
    fn comparisons_push_one_or_zero() {
        // PUSH1 5, PUSH1 3, LT  ->  3 < 5
        assert_eq!(final_stack(&[0x60, 0x05, 0x60, 0x03, 0x10]), vec![w(1)]);
        assert_eq!(final_stack(&[0x60, 0x05, 0x60, 0x03, 0x11]), vec![w(0)]);
        assert_eq!(final_stack(&[0x60, 0x05, 0x60, 0x05, 0x14]), vec![w(1)]);
        assert_eq!(final_stack(&[0x5f, 0x15]), vec![w(1)]);
        assert_eq!(final_stack(&[0x60, 0x09, 0x15]), vec![w(0)]);
    }

    #[test]
    fn bitwise_operations() {
        assert_eq!(final_stack(&[0x60, 0x0c, 0x60, 0x0a, 0x16]), vec![w(0x08)]);
        assert_eq!(final_stack(&[0x60, 0x0c, 0x60, 0x0a, 0x17]), vec![w(0x0e)]);
        assert_eq!(final_stack(&[0x60, 0x0c, 0x60, 0x0a, 0x18]), vec![w(0x06)]);
        assert_eq!(final_stack(&[0x5f, 0x19]), vec![Word::MAX]);
    }

    #[test]
    fn push_dup_swap_pop() {
        // PUSH1 1, PUSH1 2, DUP2, SWAP1, POP  ->  [1, 2, 1] -> [1, 1, 2] -> [1, 1]
        assert_eq!(
            final_stack(&[0x60, 0x01, 0x60, 0x02, 0x81, 0x90, 0x50]),
            vec![w(1), w(1)]
        );
    }

    #[test]
    fn push2_reads_two_bytes_big_endian() {
        assert_eq!(final_stack(&[0x61, 0x01, 0x02]), vec![w(0x0102)]);
    }

    #[test]
    fn truncated_push_is_padded_with_zeros() {
        // PUSH2 avec un seul octet derrière : 0x05 puis 0x00 -> 0x0500
        assert_eq!(final_stack(&[0x61, 0x05]), vec![w(0x0500)]);
        // PUSH1 sans aucun octet derrière -> 0
        assert_eq!(final_stack(&[0x60]), vec![w(0)]);
    }

    #[test]
    fn push8_and_push9_cover_both_code_paths() {
        let mut c = vec![0x67]; // PUSH8
        c.extend([0xff; 8]);
        assert_eq!(final_stack(&c), vec![w(u64::MAX)]);
        let mut c = vec![0x68]; // PUSH9
        c.extend(1..=9u8);
        assert_eq!(
            final_stack(&c),
            vec![Word::from(0x01_0203_0405_0607_0809u128)]
        );
    }

    #[test]
    fn truncated_push8_pads_on_the_right() {
        // PUSH8 avec 2 octets seulement : 0xabcd suivi de 6 zéros
        assert_eq!(
            final_stack(&[0x67, 0xab, 0xcd]),
            vec![w(0xabcd_0000_0000_0000)]
        );
        // PUSH8 sans aucun octet
        assert_eq!(final_stack(&[0x67]), vec![w(0)]);
    }

    #[test]
    fn pc_and_gas_opcodes() {
        // PUSH0 (2 gas), PC -> pousse 1
        assert_eq!(final_stack(&[0x5f, 0x58]), vec![w(0), w(1)]);
        // GAS seul : 1_000_000 - 2
        assert_eq!(final_stack(&[0x5a]), vec![w(GAS - 2)]);
    }

    #[test]
    fn add_on_empty_stack_underflows() {
        assert_eq!(
            run(&[0x01]).unwrap_err(),
            VmError::StackUnderflow {
                needed: 2,
                available: 0
            }
        );
    }

    #[test]
    fn invalid_opcode_is_an_error() {
        assert_eq!(run(&[0x0c]).unwrap_err(), VmError::InvalidOpcode(0x0c));
        assert_eq!(run(&[0xfe]).unwrap_err(), VmError::InvalidOpcode(0xfe));
    }

    #[test]
    fn stops_when_gas_runs_out() {
        // 3 PUSH1 = 9 gas, on n'en donne que 7
        let code = [0x60, 0x01, 0x60, 0x01, 0x60, 0x01];
        assert_eq!(
            Vm::new(&code, Gas::new(7)).run().unwrap_err(),
            VmError::OutOfGas {
                needed: 3,
                remaining: 1
            }
        );
    }

    /// Décode une chaîne hexadécimale en bytecode.
    fn code(hex_str: &str) -> Vec<u8> {
        hex::decode(hex_str).unwrap()
    }

    #[test]
    fn program_1_returns_2_plus_3_for_24_gas() {
        // PUSH1 2, PUSH1 3, ADD, PUSH1 0, MSTORE, PUSH1 32, PUSH1 0, RETURN
        let outcome = run(&code("600260030160005260206000f3")).unwrap();
        let mut expected = vec![0u8; 32];
        expected[31] = 5;
        assert_eq!(
            outcome,
            ExecutionOutcome::Success {
                return_data: expected,
                gas_used: 24
            }
        );
    }

    #[test]
    fn mstore_then_mload() {
        // PUSH1 42, PUSH1 0, MSTORE, PUSH1 0, MLOAD
        assert_eq!(final_stack(&code("602a600052600051")), vec![w(42)]);
    }

    #[test]
    fn mstore8_writes_only_the_low_byte() {
        // PUSH2 0x1234, PUSH1 0, MSTORE8  -> memory[0] = 0x34
        let mut vm = Vm::new(&[0x61, 0x12, 0x34, 0x60, 0x00, 0x53], Gas::new(GAS));
        vm.run().unwrap();
        assert_eq!(vm.memory().as_slice()[0], 0x34);
        assert_eq!(vm.memory().size(), 32);
    }

    #[test]
    fn msize_reports_size_in_bytes() {
        // PUSH1 0, PUSH1 33, MSTORE8, MSIZE  -> 64
        assert_eq!(final_stack(&code("600060215359")), vec![w(64)]);
    }

    #[test]
    fn memory_expansion_is_charged_once_per_word() {
        // PUSH1 1, PUSH1 0, MSTORE (3+3+3+3) puis PUSH1 1, PUSH1 0, MSTORE (3+3+3+0)
        let outcome = run(&code("60016000526001600052")).unwrap();
        assert_eq!(
            outcome,
            ExecutionOutcome::Success {
                return_data: vec![],
                gas_used: 21
            }
        );
    }

    #[test]
    fn revert_is_an_outcome_not_an_error() {
        // PUSH1 0, PUSH1 0, REVERT
        assert_eq!(
            run(&code("60006000fd")).unwrap(),
            ExecutionOutcome::Revert {
                return_data: vec![],
                gas_used: 6
            }
        );
    }

    #[test]
    fn revert_returns_data() {
        // PUSH1 0xaa, PUSH1 0, MSTORE8, PUSH1 1, PUSH1 0, REVERT  -> [0xaa]
        match run(&code("60aa60005360016000fd")).unwrap() {
            ExecutionOutcome::Revert { return_data, .. } => assert_eq!(return_data, vec![0xaa]),
            other => panic!("attendu Revert, obtenu {other:?}"),
        }
    }

    #[test]
    fn return_with_zero_size_ignores_huge_offset() {
        // PUSH0 (size), PUSH32 MAX (offset), RETURN
        let mut c = vec![0x5f, 0x7f];
        c.extend([0xff; 32]);
        c.push(0xf3);
        let mut vm = Vm::new(&c, Gas::new(GAS));
        assert_eq!(
            vm.run().unwrap(),
            ExecutionOutcome::Success {
                return_data: vec![],
                gas_used: 5
            }
        );
        assert_eq!(vm.memory().size(), 0);
    }

    #[test]
    fn mstore_at_huge_offset_is_a_clean_error() {
        // PUSH1 1, PUSH32 2^200, MSTORE
        let mut c = vec![0x60, 0x01, 0x7f];
        let mut offset = [0u8; 32];
        offset[6] = 1; // 2^200 en big-endian
        c.extend(offset);
        c.push(0x52);
        assert_eq!(run(&c).unwrap_err(), VmError::InvalidMemoryOffset);
    }

    #[test]
    fn memory_expansion_without_enough_gas_allocates_nothing() {
        // PUSH1 1, PUSH3 0x100000 (1 Mo), MSTORE : expansion d'environ 2,2 M de gas
        let c = code("60016210000052");
        let mut vm = Vm::new(&c, Gas::new(100_000));
        assert!(matches!(vm.run().unwrap_err(), VmError::OutOfGas { .. }));
        assert_eq!(vm.memory().size(), 0);
    }

    #[test]
    fn analyze_jumpdests_skips_push_data() {
        // JUMPDEST, PUSH1 0x5b, JUMPDEST
        assert_eq!(
            analyze_jumpdests(&[0x5b, 0x60, 0x5b, 0x5b]),
            vec![true, false, false, true]
        );
        // PUSH32 rempli de 0x5b : aucun n'est valide
        let mut c = vec![0x7f];
        c.extend([0x5b; 32]);
        assert!(analyze_jumpdests(&c).iter().all(|&v| !v));
    }

    #[test]
    fn program_2_countdown_loop_for_81_gas() {
        // PUSH1 3 | JUMPDEST | PUSH1 1 | SWAP1 | SUB | DUP1 | PUSH1 2 | JUMPI | STOP
        let c = code("60035b600190038060025700");
        let mut vm = Vm::new(&c, Gas::new(GAS));
        assert_eq!(
            vm.run().unwrap(),
            ExecutionOutcome::Success {
                return_data: vec![],
                gas_used: 81
            }
        );
        assert_eq!(vm.stack().as_slice(), &[Word::ZERO]);
    }

    #[test]
    fn jump_skips_code() {
        // PUSH1 4, JUMP, INVALID, JUMPDEST, PUSH1 7
        assert_eq!(final_stack(&code("600456fe5b6007")), vec![w(7)]);
    }

    #[test]
    fn jumpi_falls_through_when_condition_is_zero() {
        // PUSH0 (cond), PUSH1 6 (dest), JUMPI, PUSH1 1, JUMPDEST
        assert_eq!(final_stack(&code("5f60065760015b")), vec![w(1)]);
    }

    #[test]
    fn jump_into_push_data_is_invalid() {
        // PUSH1 0x5b, PUSH1 1, JUMP  -> la position 1 est une donnée
        assert_eq!(
            run(&code("605b600156")).unwrap_err(),
            VmError::InvalidJump(1)
        );
    }

    #[test]
    fn jump_to_non_jumpdest_is_invalid() {
        // PUSH1 3, JUMP, STOP : la position 3 est un STOP, pas un JUMPDEST
        assert_eq!(run(&code("60035600")).unwrap_err(), VmError::InvalidJump(3));
        // Destination hors du code
        assert_eq!(run(&code("606456")).unwrap_err(), VmError::InvalidJump(100));
    }

    #[test]
    fn infinite_loop_stops_on_gas() {
        // JUMPDEST, PUSH0, JUMP : 1 + 2 + 8 = 11 gas par tour
        let c = code("5b5f56");
        assert!(matches!(
            Vm::new(&c, Gas::new(10_000)).run().unwrap_err(),
            VmError::OutOfGas { .. }
        ));
    }

    #[test]
    fn stack_overflow_after_1024_pushes() {
        let code = vec![0x5f; 1025]; // 1025 x PUSH0
        assert_eq!(
            run(&code).unwrap_err(),
            VmError::StackOverflow { limit: 1024 }
        );
    }
}
