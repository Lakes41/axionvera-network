# Fuzz Testing Setup (Issue #314)

Overview
--------
This document explains how to run fuzz tests for the repository. We use `cargo-fuzz` (libFuzzer) to mutate arbitrary bytes and feed them into deserialization and parsing code paths in `network-node`.
For the vault contract reward-index math, we also use `proptest` to repeatedly generate extreme `i128` values and tiny deposit totals.

What was added
--------------
- `fuzz/` with a `Cargo.toml` and `fuzz_targets/parse_payload.rs` fuzz target that exercises `NetworkConfig` and `DatabaseConfig` deserialization and socket address parsing.
- `contracts/vault-contract` property tests that fuzz the `distribute_rewards` math around `REWARD_INDEX_SCALE`, overflow, and zero-increment edge cases.

Prerequisites
-------------
- Rust toolchain (stable) and `cargo`.
- `cargo-fuzz` installed: `cargo install cargo-fuzz`.
- For distributed/faster fuzzing: consider running on a Linux machine or in CI with adequate CPU.

Quick start (local)
-------------------
1. Install `cargo-fuzz`:

```bash
cargo install cargo-fuzz
```

2. Initialize fuzz (only needed if you want cargo-fuzz to set up its own dirs):

```bash
cd network-node
cargo fuzz init
```

3. Build and run the fuzz target from repository root (this uses the `fuzz` folder we've added):

```bash
cd fuzz
cargo fuzz run parse_payload -- -runs=1000000
```

4. Run the vault reward-index property tests:

```bash
cargo test -p axionvera-vault-contract reward_index_math
```

Notes:
- The `-runs=1000000` parameter requests 1,000,000 iterations; libFuzzer may schedule work differently depending on corpus and CPU.
- You can also run without `-runs` to let libFuzzer run indefinitely and explore inputs:

```bash
cargo fuzz run parse_payload
```

How the fuzz target works
------------------------
- The harness attempts JSON deserialization into `NetworkConfig` and `DatabaseConfig` using `serde_json::from_slice` and also attempts to parse the input as a `SocketAddr` string. Any panics, unwind, or UB encountered while exercising these code paths will be caught by libFuzzer and reported.

Interpreting results
--------------------
- If libFuzzer finds a crash, it will write a reproducer (input file) into `fuzz/artifacts/parse_payload/` and print a stack trace. Reproduce the crash by running `cargo fuzz run parse_payload -runs=1 <path-to-crash>`.

CI / automation suggestions
-------------------------
- Add a GitHub Actions workflow that installs `cargo-fuzz` and runs `cargo fuzz run parse_payload -runs=10000` as a job for PR validation (lower run counts for CI). For full local verification run `-runs=1000000`.

Limitations & follow-ups
------------------------
- The current fuzz target focuses on configuration and basic parsing functions. For deeper protocol payload fuzzing, add targets that call specific deserialization functions used for network messages (when available).
- Consider adding `asan`/`msan` builds for memory-safety checks (usually requires specific toolchains / platforms).
