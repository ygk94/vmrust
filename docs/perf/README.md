# Performances : criterion et flamegraph

Machine : Mac (Apple Silicon), `cargo bench` en profil release.

## Benchmarks criterion (`benches/interpreter.rs`)

| Benchmark | Ce qu'il mesure | Avant | Après | Gain |
|---|---|---|---|---|
| `arith_small` | `(2+3)*4`, 6 instructions | 71,1 ns | 68,5 ns | -2,9 % |
| `program_1_return` | 2+3 écrit en mémoire puis renvoyé | 124,8 ns | 118,2 ns | -5,7 % |
| `countdown/1000` | boucle de 1 000 tours (7 000 instructions) | 28,4 µs | 26,4 µs | -6,1 % |
| `countdown/10000` | boucle de 10 000 tours (70 000 instructions) | 291,0 µs | 266,5 µs | **-7,6 %** |
| `memory_loop_10k` | boucle de 10 000 tours avec MSTORE/MLOAD | 601,6 µs | 574,3 µs | -3,3 % |

Débit sur les boucles : environ **240 → 263 millions d'instructions par seconde**
(soit environ 3,8 ns par instruction après optimisation).
Le temps grandit linéairement avec le nombre de tours (100 / 1 000 / 10 000).

Reproduire :
```bash
cargo bench -- --save-baseline avant   # avant une modification
cargo bench -- --baseline avant        # après : criterion affiche la différence
open target/criterion/report/index.html
```

## Flamegraph (`examples/profile.rs`)

`examples/profile.rs` exécute en boucle le compte à rebours de 10 000 tours.

```bash
./scripts/flamegraph.sh docs/perf/flamegraph.svg   # sans root : /usr/bin/sample + inferno
cargo flamegraph --root --example profile          # alternative dtrace (mot de passe)
```

### Avant : `flamegraph-avant.svg`

- 99,8 % du temps dans `Vm::run` : `step`, `binary_op`, `Stack::push/pop` et
  `Gas::charge` y sont inlinés par le compilateur, c'est attendu pour un interpréteur.
- **9,1 % dans `memmove` / `memcpy`** (`_platform_memmove` 6,6 % + stub `memcpy` 2,5 %).
  La seule copie de la boucle chaude est `copy_from_slice` dans `read_push_data` :
  la longueur étant variable (n octets du PUSHn), le compilateur appelle `memcpy`
  pour copier 1 ou 2 octets à chaque PUSH.

### Optimisation

Dans `Vm::read_push_data`, les PUSH1 à PUSH8 (les plus fréquents de loin) sont
assemblés directement dans un `u64` octet par octet, sans copie dans un tampon de
32 octets. PUSH9 à PUSH32 gardent l'ancien chemin. Tests ajoutés pour les deux chemins
et pour les PUSH tronqués en fin de code.

### Après : `flamegraph-apres.svg`

- `memmove` / `memcpy` ont disparu du profil : il ne reste que `Vm::run` (99,9 %).
- Gain mesuré par criterion : -7,6 % sur la boucle de 10 000 tours, cohérent avec les
  ~9 % que représentait `memcpy`.

### Pistes suivantes

- Pré-décoder le bytecode une fois (`Vec<Opcode>` + données de PUSH) pour ne plus
  refaire `Opcode::from_byte` à chaque tour de boucle.
- Fusionner les vérifications de pile : `require(2)` puis deux `pop` font trois tests
  de bornes là où un seul suffit.
- Pour les petits programmes, `Vm::new` alloue la pile (1024 mots = 32 Ko) et la table
  des JUMPDEST à chaque exécution : réutiliser une `Vm` éviterait ces allocations.
