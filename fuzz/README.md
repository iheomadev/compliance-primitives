# Fuzz harnesses

This repo uses **lightweight seeded-PRNG sequence fuzzers** inside each
contract's `#[cfg(test)]` tree rather than a separate `cargo-fuzz` workspace.

## Why not cargo-fuzz / libFuzzer?

Soroban contract tests need a host `Env`, `mock_all_auths`, and `Address`
generation from `soroban-sdk` testutils. Wiring that through libFuzzer's
byte-buffer `fuzz_target!` is possible but heavy for `#![no_std]` crates and
duplicates most of the unit-test setup. A plain `#[test]` loop with a tiny
xorshift PRNG covers the same last-write-wins invariants with zero extra
toolchain deps, and the same scaffolding can be copied for denylist-gate
(#86) and jurisdiction-flag (#87).

## jurisdiction-flag (`#87`)

Harness: `contracts/jurisdiction-flag/src/fuzz.rs`

Invariants checked after every random `set_jurisdiction` sequence:

1. **Last write wins** — `get_jurisdiction(addr)` equals the most recently
   set code for that address (or `None` if never set).
2. **Permission consistency** — `is_permitted_jurisdiction(addr, allowed)`
   matches `get_jurisdiction(addr)` membership in the fuzzer's fixed
   `allowed_codes` list.

### Short run (default, also in `cargo test`)

```sh
cargo test -p jurisdiction-flag fuzz_jurisdiction_set_get_sequences -- --nocapture
```

Defaults: `FUZZ_ITERATIONS=128`, `FUZZ_OPS=24`.

### Periodic longer campaign (not in CI)

```sh
FUZZ_ITERATIONS=2000 FUZZ_OPS=64 \
  cargo test -p jurisdiction-flag fuzz_jurisdiction_set_get_sequences -- --nocapture
```

Run this after non-trivial changes to `set_jurisdiction` /
`get_jurisdiction` / `is_permitted_jurisdiction`, or as part of a release
checklist. Failures print the failing `seed` so the sequence is reproducible.
