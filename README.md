# vmrust: a mini-EVM in Rust

A stack-based virtual machine modeled on the Ethereum Virtual Machine: 256-bit words,
a 1024-item stack, expandable memory, persistent storage and gas charged for every
instruction. It runs from the command line or behind an HTTP API.

## Requirements

Rust 1.90 or newer (the minimum version required by the dependencies).

## Command line

```bash
cargo run                                      # demo: a series of sample programs
cargo run -- 600260030160005260206000f3        # run bytecode (100,000 gas by default)
cargo run -- 600260030160005260206000f3 500    # same, with 500 gas
cargo run -- 365f5f37365ff3 0x68656c6c6f       # with calldata (this program returns it)
cargo run -- trace 600260030100                # step-by-step execution
cargo run -- disasm 600260030100               # disassembler
cargo run -- serve                             # HTTP API on 127.0.0.1:3000
cargo run -- serve 127.0.0.1:8080              # HTTP API on another address
```

After the bytecode, a plain run and `trace` accept two optional arguments, in this order:
the gas limit, then the calldata in hex. The calldata can be given without the gas limit
if it starts with `0x`.

Example (the command line prints its messages in French):

```text
$ cargo run -- 600260030160005260206000f3
▶ ton bytecode
  bytecode : 600260030160005260206000f3   (gas donné : 100000)
  ✅ succès   gas utilisé : 24
  données renvoyées : 0x0000000000000000000000000000000000000000000000000000000000000005
  en décimal : 5
```

## HTTP API

| Route | Effect |
|---|---|
| `GET /health` | checks that the server is up |
| `POST /execute` | runs bytecode on a fresh state |
| `POST /contracts` | deploys code at an address |
| `POST /contracts/{address}/call` | runs the contract and keeps its storage |
| `GET /contracts/{address}/storage/{key}` | reads a storage slot (key in hex) |

Every execution returns `status` (`success`, `revert` or `error`), `gas_used`, and
either `return_data` or `error`. A program that fails still gets an HTTP 200: it is an
execution result. An invalid request gets 400, a call to an address without a contract
gets 404, and a deployment to an address already in use gets 409. Gas is capped at
30,000,000 per request.

```bash
curl -X POST localhost:3000/execute -H 'content-type: application/json' \
  -d '{"bytecode":"0x600260030160005260206000f3","gas_limit":100000}'
# {"status":"success","return_data":"0x00…05","gas_used":24}
```

`POST /execute` and `POST /contracts/{address}/call` also accept two optional fields:
`caller`, the address the program reads with `CALLER`, and `calldata`, the input data in
hex, read with `CALLDATALOAD`, `CALLDATASIZE` and `CALLDATACOPY`.

A contract that adds the number passed as calldata to slot 0
(`PUSH0 SLOAD PUSH0 CALLDATALOAD ADD PUSH0 SSTORE`):

```bash
curl -X POST localhost:3000/contracts -H 'content-type: application/json' \
  -d '{"address":"0x00000000000000000000000000000000000000aa","code":"5f545f35015f55"}'
curl -X POST localhost:3000/contracts/0x00000000000000000000000000000000000000aa/call \
  -H 'content-type: application/json' \
  -d '{"gas_limit":100000,"calldata":"0x0000000000000000000000000000000000000000000000000000000000000005"}'
curl localhost:3000/contracts/0x00000000000000000000000000000000000000aa/storage/0
# {"value":"0x00…05"}
```

## Instructions

| Hex | Instructions | Gas |
|---|---|---|
| `00` | STOP | 0 |
| `01`, `03` | ADD, SUB | 3 |
| `02`, `04`, `06` | MUL, DIV, MOD | 5 |
| `0a` | EXP | 10 + 50 per byte of the exponent |
| `10`, `11`, `14`, `15` | LT, GT, EQ, ISZERO | 3 |
| `16` to `19` | AND, OR, XOR, NOT | 3 |
| `1b`, `1c` | SHL, SHR | 3 |
| `20` | KECCAK256 | 30 + 6 per hashed word + memory |
| `33` | CALLER | 2 |
| `35` | CALLDATALOAD | 3 |
| `36` | CALLDATASIZE | 2 |
| `37` | CALLDATACOPY | 3 + 3 per copied word + memory |
| `50` | POP | 2 |
| `51` to `53` | MLOAD, MSTORE, MSTORE8 | 3 + memory |
| `54` | SLOAD | 2,100 |
| `55` | SSTORE | 20,000 to fill an empty slot, 5,000 otherwise |
| `56`, `57` | JUMP, JUMPI | 8, 10 |
| `58` to `5a` | PC, MSIZE, GAS | 2 |
| `5b` | JUMPDEST | 1 |
| `5f` | PUSH0 | 2 |
| `60` to `7f` | PUSH1 to PUSH32 | 3 |
| `80` to `8f` | DUP1 to DUP16 | 3 |
| `90` to `9f` | SWAP1 to SWAP16 | 3 |
| `f3`, `fd` | RETURN, REVERT | 0 + memory |
| `fe` | INVALID | error |

Main rules:

- arithmetic is modulo 2^256, and a division or modulo by zero gives 0;
- reading past the end of the calldata gives zeros;
- growing memory costs the difference in `3 × words + words² / 512` between the new
  and the old size, paid before allocating;
- an error (unknown opcode, stack, gas, invalid jump or memory offset) consumes all the
  gas;
- REVERT is not an error: it returns its data and the gas actually used;
- storage writes are applied only when the execution succeeds.

## Code layout

| File | Role |
|---|---|
| `src/interpreter.rs` | execution loop and journal of storage writes |
| `src/opcode.rs` | opcode decoding |
| `src/gas.rs` | gas costs |
| `src/stack.rs` | stack |
| `src/memory.rs` | memory |
| `src/state.rs` | persistent state: `StateBackend` trait and `InMemoryState` |
| `src/types.rs` | `Word`, `Gas`, `Address`, `ProgramCounter` |
| `src/error.rs` | VM errors |
| `src/disasm.rs` | disassembler |
| `src/api.rs` | HTTP API (axum) |
| `src/main.rs` | command line |

## Tests and performance

```bash
cargo test --all-targets
cargo clippy --all-targets -- -D warnings
cargo bench
```

The tests cover every module, the HTTP API and complete programs
(`tests/programs.rs`). Benchmark results and flamegraphs are in
[`docs/perf/README.md`](docs/perf/README.md).

## Limitations

- Storage and contracts live in memory and are lost when the server stops.
- The API has no authentication: anyone can deploy and choose their `caller`.
- The total size of the storage is not limited.
- Contract calls run one at a time, and a storage read waits for the call in progress
  to finish.
- SLOAD and SSTORE are simplified: no cold or warm access, no refunds.
- No calls between contracts and no signed arithmetic.
