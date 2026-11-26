# Performance: criterion and flamegraph

Machine: Mac (Apple Silicon), `cargo bench` with the release profile.

## Criterion benchmarks (`benches/interpreter.rs`)

| Benchmark | What it measures | Before | After | Gain |
|---|---|---|---|---|
| `arith_small` | `(2+3)*4`, 6 instructions | 71.1 ns | 68.5 ns | -2.9% |
| `program_1_return` | 2+3 written to memory, then returned | 124.8 ns | 118.2 ns | -5.7% |
| `countdown/1000` | 1,000-iteration loop (7,000 instructions) | 28.4 µs | 26.4 µs | -6.1% |
| `countdown/10000` | 10,000-iteration loop (70,000 instructions) | 291.0 µs | 266.5 µs | **-7.6%** |
| `memory_loop_10k` | 10,000-iteration loop with MSTORE/MLOAD | 601.6 µs | 574.3 µs | -3.3% |

Loop throughput: about **240 → 263 million instructions per second**
(about 3.8 ns per instruction after the optimization).
Time grows linearly with the number of iterations (100 / 1,000 / 10,000).

To reproduce:
```bash
cargo bench --bench interpreter -- --save-baseline before   # before a change
cargo bench --bench interpreter -- --baseline before        # after: criterion shows the difference
open target/criterion/report/index.html
```

## Flamegraph (`examples/profile.rs`)

`examples/profile.rs` runs the 10,000-iteration countdown over and over.

```bash
./scripts/flamegraph.sh docs/perf/flamegraph.svg   # no root needed: /usr/bin/sample + inferno
cargo flamegraph --root --example profile          # dtrace alternative (asks for a password)
```

### Before: `flamegraph-before.svg`

- 99.8% of the time is spent in `Vm::run`: `step`, `binary_op`, `Stack::push/pop` and
  `Gas::charge` are inlined into it by the compiler, as expected for an interpreter.
- **9.1% in `memmove` / `memcpy`** (`_platform_memmove` 6.6% + `memcpy` stub 2.5%).
  The only copy in the hot loop is `copy_from_slice` in `read_push_data`: since the
  length varies (the n bytes of PUSHn), the compiler calls `memcpy` to copy 1 or 2
  bytes on every PUSH.

### Optimization

In `Vm::read_push_data`, PUSH1 to PUSH8 (by far the most frequent) are assembled
directly into a `u64`, byte by byte, without copying into a 32-byte buffer. PUSH9 to
PUSH32 keep the previous path. Tests were added for both paths and for PUSH
instructions truncated at the end of the code.

### After: `flamegraph-after.svg`

- `memmove` / `memcpy` are gone from the profile: only `Vm::run` remains (99.9%).
- Gain measured by criterion: -7.6% on the 10,000-iteration loop, consistent with the
  ~9% that `memcpy` accounted for.

### Next steps

- Decode the bytecode once (`Vec<Opcode>` + PUSH data) so that `Opcode::from_byte`
  no longer runs on every iteration.
- Merge the stack checks: `require(2)` followed by two `pop` calls makes three bounds
  checks where one would do.
- For small programs, `Vm::new` allocates the stack (1024 words = 32 KiB) and the
  JUMPDEST table on every run: reusing a `Vm` would avoid these allocations.

## After adding storage

First measurement, on a Windows PC: median of 7 alternating runs, before (`bbcd6fe`)
and after (`e69f695`) adding storage and the new opcodes.

| Benchmark | Before | After | Difference |
|---|---|---|---|
| `countdown/100` | 3.17 µs | 3.12 µs | noise |
| `countdown/1000` | 30.0 µs | 30.4 µs | noise |
| `countdown/10000` | 298.9 µs | 290.4 µs | noise |
| `memory_loop_10k` | 554.7 µs | 664.4 µs | +20% |
| `program_1_return` | 262.7 ns | 296.9 ns | +13% |
| `arith_small` | 185.8 ns | 198.4 ns | about +7%, noisy |

Second measurement, on another Windows PC with Rust 1.100 (`x86_64-pc-windows-gnu`).
The CPU of this machine switches between two speeds, about three times apart, from one
run to the next, so absolute times cannot be compared. Each version is built in its own
target directory and the versions run in turn, 6 rounds in a rotating order; the table
gives the median of the per-round differences.

| Benchmark | Difference | Slower in |
|---|---|---|
| `countdown/100` | +7.7% | 4 of 6 rounds |
| `countdown/1000` | +5.5% | 4 of 6 rounds |
| `countdown/10000` | +8.1% | 3 of 6 rounds |
| `memory_loop_10k` | +37.6% | 6 of 6 rounds |
| `program_1_return` | +22.8% | 5 of 6 rounds |
| `arith_small` | +13.1% | 4 of 6 rounds |

`examples/profile.rs` gives a finer view of the countdown loop: 60 rounds of short runs
of the two versions in turn give +3.6% (slower in 45 of 60 rounds).

The slowdown is confirmed, mostly on memory accesses (`memory_loop_10k`) and on short
programs (`program_1_return`). It has not been fixed yet.

Two checkouts that share one `CARGO_TARGET_DIR` cannot be compared this way: cargo can
treat the binary built from one as up to date for the other and benchmark the same code
twice. Switching commits in a single checkout, as below, does not have this problem:

```bash
git checkout bbcd6fe && cargo bench --bench interpreter -- --save-baseline before-storage
git checkout main && cargo bench --bench interpreter -- --baseline before-storage
```

## After adding CALLDATA

Same Windows PC and method, `e69f695` against the version with CALLDATA.

The first version of CALLDATA made the countdown loop of `examples/profile.rs` 19.8%
slower (slower in 55 of 60 rounds). The disassembly showed why: with three more opcodes,
`Opcode::from_byte` was no longer inlined in that binary, so the loop called it on every
instruction. Marking it `#[inline]`, like the other functions of the hot loop, brings the
decoding back into the loop, and the same measurement then gives -5.1% (faster in 47 of
60 rounds).

Criterion, after this fix:

| Benchmark | Difference | Slower in |
|---|---|---|
| `countdown/100` | +7.0% | 4 of 6 rounds |
| `countdown/1000` | +6.8% | 4 of 6 rounds |
| `countdown/10000` | +11.3% | 6 of 6 rounds |
| `memory_loop_10k` | -4.2% | 0 of 6 rounds |
| `program_1_return` | -1.0% | 3 of 6 rounds |
| `arith_small` | +6.8% | 3 of 6 rounds |

In the benchmark binary, `from_byte` was already inlined in both versions and the loop
has the same structure, so the remaining differences most likely come from where the
code is placed: the same change makes the countdown loop 5.1% faster in
`examples/profile.rs` and up to 11% slower here.
