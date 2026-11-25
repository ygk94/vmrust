# Branche `feat/state-api` : changements et reste à faire

Base : `bb4e598` (après la PR #5). Tests : 89 → 116, clippy sans avertissement.

## Changements

### Storage et nouveaux opcodes

- `state.rs` : trait `StateBackend` et implémentation en RAM `InMemoryState`
  (code et storage séparés par adresse, écrire 0 libère la case).
- `interpreter.rs` :
  - `Vm` est générique sur l'état. `Vm::new` s'utilise comme avant ;
    `Vm::with_state` ajoute l'état, l'adresse du contrat et l'appelant.
  - Journal : SSTORE écrit dans une table temporaire, appliquée à l'état en cas de
    succès et jetée sur REVERT ou erreur. SLOAD lit d'abord le journal.
  - Après une erreur, `gas_remaining()` vaut 0 (avant, le reste n'était pas effacé).
  - `run_with` appelle une fonction avant chaque instruction (mode trace).
- `types.rs` : conversions texte → `Address` et `Address` → `Word`.
- `opcode.rs`, `gas.rs` : sept opcodes.

| Hex | Opcode | Gas |
|---|---|---|
| 0a | EXP | 10 + 50 par octet de l'exposant |
| 1b, 1c | SHL, SHR | 3 |
| 20 | KECCAK256 | 30 + 6 par mot + mémoire |
| 33 | CALLER | 2 |
| 54 | SLOAD | 2100 |
| 55 | SSTORE | 20000 (case vide → non vide), sinon 5000 |

### API HTTP (`api.rs`)

| Route | Effet |
|---|---|
| `GET /health` | le serveur répond |
| `POST /execute` | exécute du bytecode sur un état vierge |
| `POST /contracts` | installe du code à une adresse |
| `POST /contracts/{address}/call` | exécute le contrat, storage conservé |
| `GET /contracts/{address}/storage/{key}` | lit une case |

- Requête invalide : HTTP 4xx. Programme qui échoue : HTTP 200 avec `status: "error"`.
- 30 M de gas maximum par requête, pour qu'une boucle infinie s'arrête.
- Les appels de contrats passent un par un. Le verrou est async : une requête qui attend
  ne bloque pas le serveur.

### Ligne de commande

```bash
cargo run                              # démo, inchangée
cargo run -- <hex> [gas]               # inchangé
cargo run -- trace <hex> [gas]         # exécution pas à pas
cargo run -- disasm <hex>              # désassembleur (disasm.rs)
cargo run -- serve [adresse:port]      # API, 127.0.0.1:3000 par défaut
```

## Performance

Médiane de 7 passages alternés par version, sur un PC Windows bruité. À ne pas mélanger
avec `docs/perf/README.md`, mesuré sur un Mac.

| Benchmark | Avant | Après | Écart |
|---|---|---|---|
| `countdown/100` | 3,17 µs | 3,12 µs | bruit |
| `countdown/1000` | 30,0 µs | 30,4 µs | bruit |
| `countdown/10000` | 298,9 µs | 290,4 µs | bruit |
| `memory_loop_10k` | 554,7 µs | 664,4 µs | **+20 %** |
| `program_1_return` | 262,7 ns | 296,9 ns | **+13 %** |
| `arith_small` | 185,8 ns | 198,4 ns | environ +7 %, bruité |

## Reste à faire

### Avant la présentation

- [ ] Corriger le ralentissement mémoire et démarrage. Pistes, non vérifiées : sortir de
      `step` les opcodes lourds (`#[inline(never)]`), ne créer le journal qu'au premier
      SSTORE. Confirmer au flamegraph.
- [ ] Relire et merger la branche.
- [ ] Slides : storage fait (p. 1), ajouter `state` et `api` (p. 2), 116 tests et coûts
      SLOAD/SSTORE simplifiés (p. 3).

### Optionnel

- [ ] README du projet.
- [ ] Refaire les mesures de `docs/perf/README.md`.
- [ ] Comparaison avec revm, sur le même bytecode et la même machine.
- [ ] CALLDATA, pour passer des paramètres à un contrat.
- [ ] Storage sur disque.

## Limites connues

- Le storage et les contrats sont perdus à l'arrêt du serveur.
- API sans authentification : chacun peut déployer et choisir son `caller`.
- La taille totale du storage n'est pas limitée.
- SLOAD/SSTORE simplifiés : pas d'accès froid/chaud ni de remboursement.
- Les lectures de storage attendent la fin de l'appel de contrat en cours.
- Pas d'appels entre contrats ni d'arithmétique signée.
