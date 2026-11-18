//! Pile de la VM : mots de 256 bits, 1024 éléments maximum.
//!
//! Le haut de la pile est la FIN du `Vec` : `Vec::push` et `Vec::pop` y sont en O(1).

use crate::error::{Result, VmError};
use crate::types::Word;

/// Nombre maximum d'éléments sur la pile (limite de l'EVM).
pub const STACK_LIMIT: usize = 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stack {
    items: Vec<Word>,
}

impl Default for Stack {
    fn default() -> Self {
        Self::new()
    }
}

impl Stack {
    /// Pile vide, avec la place pour 1024 mots réservée d'un coup : plus aucune
    /// réallocation pendant l'exécution.
    pub fn new() -> Self {
        Stack {
            items: Vec::with_capacity(STACK_LIMIT),
        }
    }

    /// Empile `value`. Erreur si la pile contient déjà 1024 éléments.
    #[inline]
    pub fn push(&mut self, value: Word) -> Result<()> {
        if self.items.len() >= STACK_LIMIT {
            return Err(VmError::StackOverflow { limit: STACK_LIMIT });
        }
        self.items.push(value);
        Ok(())
    }

    /// Dépile et renvoie le haut. Erreur si la pile est vide.
    #[inline]
    pub fn pop(&mut self) -> Result<Word> {
        self.items.pop().ok_or(VmError::StackUnderflow {
            needed: 1,
            available: 0,
        })
    }

    /// Vérifie qu'il y a au moins `n` éléments, pour signaler un underflow précis
    /// (« needed 2, had 1 ») avant de dépiler quoi que ce soit.
    #[inline]
    pub fn require(&self, n: usize) -> Result<()> {
        let len = self.items.len();
        if len < n {
            return Err(VmError::StackUnderflow {
                needed: n,
                available: len,
            });
        }
        Ok(())
    }

    /// Regarde sans dépiler. `depth = 0` est le haut, 1 l'élément juste en dessous, etc.
    pub fn peek(&self, depth: usize) -> Result<&Word> {
        let len = self.items.len();
        if depth >= len {
            return Err(VmError::StackUnderflow {
                needed: depth + 1,
                available: len,
            });
        }
        Ok(&self.items[len - 1 - depth])
    }

    /// DUPn : copie le n-ième élément en partant du haut (DUP1 = le haut) et l'empile.
    pub fn dup(&mut self, n: usize) -> Result<()> {
        let len = self.items.len();
        if n == 0 || len < n {
            return Err(VmError::StackUnderflow {
                needed: n,
                available: len,
            });
        }
        let value = self.items[len - n];
        self.push(value)
    }

    /// SWAPn : échange le haut avec l'élément situé n places en dessous
    /// (SWAP1 échange les deux premiers).
    pub fn swap(&mut self, n: usize) -> Result<()> {
        let len = self.items.len();
        if n == 0 || len < n + 1 {
            return Err(VmError::StackUnderflow {
                needed: n + 1,
                available: len,
            });
        }
        self.items.swap(len - 1, len - 1 - n);
        Ok(())
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Vue sur le contenu, du bas vers le haut (le haut est le dernier élément).
    pub fn as_slice(&self) -> &[Word] {
        &self.items
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn w(n: u64) -> Word {
        Word::from(n)
    }

    /// Pile remplie avec 1, 2, ..., n (n en haut).
    fn stack_of(n: u64) -> Stack {
        let mut s = Stack::new();
        for i in 1..=n {
            s.push(w(i)).unwrap();
        }
        s
    }

    #[test]
    fn push_then_pop_is_lifo() {
        let mut s = stack_of(3);
        assert_eq!(s.pop().unwrap(), w(3));
        assert_eq!(s.pop().unwrap(), w(2));
        assert_eq!(s.pop().unwrap(), w(1));
        assert!(s.is_empty());
    }

    #[test]
    fn pop_on_empty_stack_underflows() {
        let mut s = Stack::new();
        assert_eq!(
            s.pop().unwrap_err(),
            VmError::StackUnderflow {
                needed: 1,
                available: 0
            }
        );
    }

    #[test]
    fn push_1025th_item_overflows() {
        let mut s = stack_of(STACK_LIMIT as u64);
        assert_eq!(s.len(), 1024);
        assert_eq!(
            s.push(w(0)).unwrap_err(),
            VmError::StackOverflow { limit: 1024 }
        );
        assert_eq!(s.len(), 1024);
    }

    #[test]
    fn require_reports_needed_and_available() {
        let s = stack_of(1);
        assert!(s.require(1).is_ok());
        assert_eq!(
            s.require(2).unwrap_err(),
            VmError::StackUnderflow {
                needed: 2,
                available: 1
            }
        );
    }

    #[test]
    fn peek_reads_without_popping() {
        let s = stack_of(3);
        assert_eq!(*s.peek(0).unwrap(), w(3));
        assert_eq!(*s.peek(2).unwrap(), w(1));
        assert!(s.peek(3).is_err());
        assert_eq!(s.len(), 3);
    }

    #[test]
    fn dup1_copies_the_top() {
        let mut s = stack_of(2);
        s.dup(1).unwrap();
        assert_eq!(s.as_slice(), &[w(1), w(2), w(2)]);
    }

    #[test]
    fn dup3_copies_the_third_from_top() {
        let mut s = stack_of(4); // [1, 2, 3, 4]
        s.dup(3).unwrap();
        assert_eq!(s.as_slice(), &[w(1), w(2), w(3), w(4), w(2)]);
    }

    #[test]
    fn dup16_needs_16_items() {
        let mut s = stack_of(15);
        assert_eq!(
            s.dup(16).unwrap_err(),
            VmError::StackUnderflow {
                needed: 16,
                available: 15
            }
        );
        let mut s = stack_of(16);
        s.dup(16).unwrap();
        assert_eq!(*s.peek(0).unwrap(), w(1));
    }

    #[test]
    fn dup_on_full_stack_overflows() {
        let mut s = stack_of(STACK_LIMIT as u64);
        assert_eq!(
            s.dup(1).unwrap_err(),
            VmError::StackOverflow { limit: 1024 }
        );
    }

    #[test]
    fn swap1_exchanges_the_two_top_items() {
        let mut s = stack_of(3);
        s.swap(1).unwrap();
        assert_eq!(s.as_slice(), &[w(1), w(3), w(2)]);
    }

    #[test]
    fn swap3_exchanges_top_and_fourth() {
        let mut s = stack_of(4); // [1, 2, 3, 4]
        s.swap(3).unwrap();
        assert_eq!(s.as_slice(), &[w(4), w(2), w(3), w(1)]);
    }

    #[test]
    fn swap_needs_n_plus_one_items() {
        let mut s = stack_of(1);
        assert_eq!(
            s.swap(1).unwrap_err(),
            VmError::StackUnderflow {
                needed: 2,
                available: 1
            }
        );
    }
}
