# jurisdiction-denylist-consumer

Reference example contract demonstrating composition of both `denylist-gate` and `jurisdiction-flag` in a consuming token contract's transfer function.

## Overview

This contract demonstrates how an issuer can compose multiple compliance primitives together prior to executing balance mutations.

### Check Order

During a `transfer(from, to, amount)` invocation, the checks are executed in the following strict order:

1. **Denylist Check (`denylist-gate`)**:
   Calls `check()` for both `from` (sender) and `to` (recipient).
   - If either address is denylisted, the contract immediately aborts with `Error::DeniedByGate`.

2. **Jurisdiction Flag Check (`jurisdiction-flag`)**:
   Calls `is_permitted_jurisdiction()` for `from` (sender) against the contract's configured `allowed_jurisdictions` list.
   - If the sender's address has no jurisdiction set or its jurisdiction code is not in the allowed list, the contract aborts with `Error::DeniedByJurisdiction`.

3. **Balance Check & Mutation**:
   - Verifies `from` has sufficient balance (`Error::InsufficientBalance`).
   - Updates `from` and `to` balances upon success.

## Trait-based Client Generation

To avoid link-time WASM symbol collisions, this contract does not include direct crate dependencies for `denylist-gate` or `jurisdiction-flag` in its core build dependencies. Instead, it uses `#[contractclient]` trait declarations (`GateClient` and `JurisdictionFlagClient`) to interact with deployed instances via cross-contract calls.
