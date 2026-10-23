//! Types du domaine : mots de 256 bits, gas, adresses, compteur de programme.

//explique tous les uses
// R: `use` importe un nom pour pouvoir l'écrire en court dans ce fichier. Les chemins
//    disent d'où vient chaque nom :
//    - `std::fmt` : module de la bibliothèque standard pour l'affichage. On s'en sert
//      plus bas pour `impl fmt::Display for Address`.
//    - `ruint::aliases::U256` : le type entier non signé de 256 bits, fourni par le
//      crate `ruint` (Rust n'a que u128 en natif). C'est la taille d'un mot EVM.
//    - `serde::{Deserialize, Serialize}` : les macros derive qui permettent de convertir
//      Gas et Address vers/depuis du JSON (pour l'API axum).
//    - `crate::error::{Result, VmError}` : `crate` = la racine de NOTRE projet. On importe
//      notre alias Result et notre enum d'erreur, définis dans src/error.rs.
use std::fmt;

use ruint::aliases::U256;
use serde::{Deserialize, Serialize};

use crate::error::{Result, VmError};

pub type Word = U256; //pourquoi un grand U ici ?
// R: Convention Rust : les TYPES s'écrivent en CamelCase avec une majuscule (Gas,
//    Address, VmError, String, Vec). Seuls les types primitifs du langage (u8, u64,
//    usize, bool) sont en minuscules. U256 n'est pas un primitif : c'est une struct
//    définie par le crate ruint, donc majuscule. `type Word = U256;` est un alias :
//    Word et U256 désignent exactement le même type, Word est juste un nom plus parlant.

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Gas(u64);

impl Gas {
    pub fn new(amount: u64) -> Self {
        Gas(amount)
    }

    pub fn value(self) -> u64 {
        self.0 //c'est quoi le self.0 ? on set le gas à 0 ?
        // R: Non, on ne met rien à 0. `.0` veut dire « le champ n°0 ». Gas est une tuple
        //    struct `Gas(u64)` : ses champs n'ont pas de nom, on y accède par position
        //    (.0, .1, ...). Il n'y en a qu'un, donc self.0 = le u64 caché dans le Gas.
        //    Cette fonction le LIT et le renvoie : Gas::new(10).value() renvoie 10.
    }

    //ici c'est charge dans le sens de charger un prix pas comme charger de la batterie c'est ça ?
    // R: Oui : « facturer » (to charge someone = faire payer). `self` est le gas restant,
    //    `cost` le prix de l'opcode. Si le prix dépasse ce qui reste, on renvoie OutOfGas
    //    SANS toucher au gas. Sinon on le décrémente. `&mut self` : la méthode a le droit
    //    de modifier le Gas sur lequel on l'appelle.
    pub fn charge(&mut self, cost: Gas) -> Result<()> {
        if cost.0 > self.0 {
            return Err(VmError::OutOfGas {
                needed: cost.0,
                remaining: self.0,
            });
        }
        self.0 -= cost.0;
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ProgramCounter(usize);

impl ProgramCounter {
    // à quoi sert prgramme counter ? et chaque fn inside
    // R: Le program counter (pc) est la position de l'octet que la VM va lire dans le
    //    bytecode, comme un doigt posé sur une ligne de texte. Il démarre à 0
    //    (derive Default). La boucle de l'interpréteur fait : lire code[pc], exécuter,
    //    déplacer pc. Les 3 méthodes :
    //    - get : lire la position actuelle (pour faire code[pc.get()]).
    //    - advance(n) : avancer de n octets. 1 pour un opcode normal, 1 + n pour un
    //      PUSHn (l'opcode + ses n octets de donnée). Ex : après `60 05`, advance(2).
    //    - jump_to(dest) : voir plus bas.
    //    Un newtype plutôt qu'un simple usize : impossible de faire pc * 3 ou de
    //    le confondre avec une taille de pile, seuls ces 3 mouvements existent.
    pub fn get(self) -> usize {
        self.0
    }

    pub fn advance(&mut self, n: usize) {
        self.0 += n;
    }

    pub fn jump_to(&mut self, destination: usize) {
        //à quoi sert je jum_to ?
        // R: À exécuter JUMP et JUMPI : au lieu d'avancer à l'octet suivant, le pc est
        //    placé directement à `destination` (prise sur la pile). C'est ce qui permet
        //    les boucles et les if. La vérification « la destination est un JUMPDEST »
        //    se fera dans l'interpréteur, qui renverra InvalidJump sinon.
        self.0 = destination;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct Address([u8; 20]);

impl From<Word> for Address {
    //ke word correspond à une adresse ? from word c'est que ca veint d'un trait c'est ça ?
    // R: Deux réponses.
    //    1) Oui, `From` est un trait de la bibliothèque standard : « comment construire
    //       ce type à partir d'un autre ». L'implémenter permet d'écrire
    //       Address::from(word) ou word.into().
    //    2) Un Word n'EST pas une adresse, mais dans l'EVM les adresses circulent sur la
    //       pile sous forme de Word (32 octets), alignées à droite. Exemple : CALLER
    //       pousse l'adresse de l'appelant sur la pile ; pour la récupérer en Address on
    //       garde les 20 derniers octets et on ignore les 12 premiers (des zéros).
    fn from(word: Word) -> Self {
        let bytes: [u8; 32] = word.to_be_bytes();
        let mut address = [0u8; 20]; //pourquoi adress est un mut, faut pas qu'on y touche nan ? ou justement on l'impélmente la dedasn ?
        // genre au debut il est vide et dcp on doit le modfier ?
        // R: Exactement ton intuition. On crée d'abord un tableau de 20 zéros, puis on le
        //    REMPLIT à la ligne suivante avec copy_from_slice. Remplir = modifier, donc mut.
        //    En Rust, toute variable est non modifiable par défaut ; `mut` est obligatoire
        //    dès qu'on la change. Ce `mut` est local : une fois l'Address construite et
        //    renvoyée, plus personne ne peut la modifier (son champ n'est pas pub).
        address.copy_from_slice(&bytes[12..]);
        Address(address)
    }
}

impl fmt::Display for Address {
    //goal de cette implementation pour adresse, on ajoute un trait dessus c'est ça ? danq quel but ?
    // R: Oui, on implémente le trait standard Display pour Address. Display = « comment
    //    afficher ce type pour un humain ». Une fois implémenté, on peut écrire
    //    println!("{address}") ou address.to_string() et obtenir 0x0000...abcd, le format
    //    habituel d'une adresse Ethereum, au lieu de [0, 0, ..., 171, 205] (ce qu'afficherait
    //    Debug). Utile pour les logs, les messages d'erreur et le test plus bas.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "0x{}", hex::encode(self.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn charge_subtracts_cost_from_remaining_gas() {
        let mut gas = Gas::new(10);
        gas.charge(Gas::new(3)).unwrap();
        assert_eq!(gas.value(), 7);
    }

    #[test]
    fn charge_fails_when_cost_exceeds_remaining() {
        let mut gas = Gas::new(1);
        let err = gas.charge(Gas::new(5)).unwrap_err();
        assert_eq!(
            err,
            VmError::OutOfGas {
                needed: 5,
                remaining: 1
            }
        );
        assert_eq!(gas.value(), 1);
    }

    #[test]
    fn program_counter_advances_and_jumps() {
        let mut pc = ProgramCounter::default();
        pc.advance(2);
        assert_eq!(pc.get(), 2);
        pc.jump_to(10);
        assert_eq!(pc.get(), 10);
    }

    #[test]
    fn address_keeps_the_last_20_bytes_of_a_word() {
        let word = Word::from(0xabcdu64);
        let address = Address::from(word);
        assert_eq!(
            address.to_string(),
            "0x000000000000000000000000000000000000abcd"
        );
    }
}
