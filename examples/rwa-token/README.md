# RWA Token Reference Implementation

A complete example of how to compose all three compliance primitives into a single reference token contract.

## Overview

This contract demonstrates how issuers can integrate:
- **Denylist gating** — block transactions from/to sanctioned addresses
- **Allowlist gating** — restrict transfers to KYC-verified addresses
- **Jurisdiction checks** — enforce regional restrictions

into a single `transfer` function, following the same `#[contractclient]` composition pattern used by the primitive contracts themselves.

## Architecture

### Contract References

Instead of depending directly on the primitive contract crates (which would cause wasm export collisions), this contract uses trait-based client generation:

```rust
#[contractclient(name = "DenylistClient")]
pub trait DenylistInterface {
    fn check(env: Env, address: Address) -> bool;
}
```

This generates a client that can invoke the deployed denylist-gate contract via cross-contract call, without linking its wasm code. Same pattern for allowlist and jurisdiction.

### Initialization

```rust
token.initialize(
    admin,              // Manages mint/burn and primitive configuration
    denylist_addr,      // Deployed denylist-gate contract
    allowlist_addr,     // Deployed allowlist gate (or allowlist-token wrapper)
    jurisdiction_addr   // Deployed jurisdiction-flag contract
);
```

All four addresses are stored in instance storage.

## Transfer Check Sequence

The `transfer` function performs checks in this order:

### 1. Denylist Check (Fastest)
```rust
if !denylist.check(&from) || !denylist.check(&to) {
    return Err(Error::DeniedByDenylist);
}
```
Both sender and recipient must NOT be on the denylist. This is the fastest check (single storage lookup per address).

### 2. Allowlist Check
```rust
if !allowlist.is_allowed(&from) || !allowlist.is_allowed(&to) {
    return Err(Error::NotOnAllowlist);
}
```
Both sender and recipient must be on the allowlist (KYC-verified, for example).

### 3. Jurisdiction Check
```rust
if !jurisdiction.is_permitted_jurisdiction(&from, &allowed_codes) ||
   !jurisdiction.is_permitted_jurisdiction(&to, &allowed_codes) {
    return Err(Error::NotPermittedJurisdiction);
}
```
Both addresses must be in permitted jurisdictions. In this reference implementation, this check is skipped if `allowed_codes` is empty (allowing all jurisdictions).

### 4. Balance Check & Transfer
If all compliance checks pass:
```rust
// Check sender has sufficient balance
// Debit sender, credit recipient
```

## Error Codes

Each step has a distinct error code, so callers can identify which gate blocked the transfer:

| Error | Code | Meaning |
|-------|------|---------|
| `DeniedByDenylist` | 5 | Sender or recipient is on the denylist |
| `NotOnAllowlist` | 6 | Sender or recipient is not on the allowlist |
| `NotPermittedJurisdiction` | 7 | Sender or recipient is not in an allowed jurisdiction |
| `InsufficientBalance` | 4 | Sender has insufficient funds |

This allows wallets/apps to provide user-friendly error messages or retry logic.

## Example Usage

### 1. Deploy all four contracts
```bash
stellar contract deploy --wasm denylist-gate.wasm --source issuer --network testnet
# → denylist_addr

stellar contract deploy --wasm allowlist-token.wasm --source issuer --network testnet
# → allowlist_addr

stellar contract deploy --wasm jurisdiction-flag.wasm --source issuer --network testnet
# → jurisdiction_addr

stellar contract deploy --wasm rwa-token.wasm --source issuer --network testnet
# → rwa_token_addr
```

### 2. Initialize the RWA token
```bash
stellar contract invoke \
  --id rwa_token_addr \
  --source issuer \
  --network testnet \
  -- initialize \
  --admin issuer \
  --denylist-gate denylist_addr \
  --allowlist-gate allowlist_addr \
  --jurisdiction-flag jurisdiction_addr
```

### 3. Mint tokens
```bash
stellar contract invoke \
  --id rwa_token_addr \
  --source issuer \
  --network testnet \
  -- mint \
  --admin issuer \
  --to alice_addr \
  --amount 1000000
```

### 4. Allow addresses and transfer
```bash
# Add alice and bob to allowlist
stellar contract invoke --id allowlist_addr --source issuer -- add_to_allowlist --admin issuer --address alice_addr
stellar contract invoke --id allowlist_addr --source issuer -- add_to_allowlist --admin issuer --address bob_addr

# Set jurisdictions
stellar contract invoke --id jurisdiction_addr --source issuer -- set_jurisdiction --issuer issuer --address alice_addr --code "US"
stellar contract invoke --id jurisdiction_addr --source issuer -- set_jurisdiction --issuer issuer --address bob_addr --code "CA"

# Transfer
stellar contract invoke \
  --id rwa_token_addr \
  --source alice_addr \
  --network testnet \
  -- transfer \
  --from alice_addr \
  --to bob_addr \
  --amount 100000
```

## Testing

This example includes comprehensive tests:

```bash
cargo test -p rwa-token
```

### Test Coverage

- ✅ Mint increases balance
- ✅ Non-admin cannot mint
- ✅ Transfer fails if sender is denied
- ✅ Transfer fails if recipient is denied
- ✅ Transfer fails if insufficient balance
- ✅ Transfer succeeds when all checks pass
- ✅ Transfer fails if sender not on allowlist
- ✅ Multiple checks fail in the correct order (first-failure determinism)

## Security Considerations

### Storage Compatibility

This example assumes all three primitives maintain stable storage layouts. If an upgraded version of a primitive changes its storage schema, this contract will continue to call the old interface but may get unexpected results.

### Jurisdiction Logic

In this reference implementation, jurisdiction checks are simplified (empty allowed_codes = all allowed). A production implementation should:
- Store the list of allowed jurisdictions in contract storage
- Allow the admin to update the allowed jurisdictions via a separate function
- Emit events when jurisdiction restrictions change

### Composability Guarantee

This contract does NOT depend directly on the primitive crates, only on trait definitions. This means:
- Primitives can be upgraded without recompiling this contract's wasm
- This contract can call any contract that implements the `DenylistInterface`, `AllowlistInterface`, or `JurisdictionInterface` traits
- Malicious contracts that claim to implement these interfaces could be used, so the admin must carefully initialize with the correct contract addresses

### Check Order Efficiency

Checks are performed in this order:
1. **Denylist** — O(1) lookup, early rejection for high-risk addresses
2. **Allowlist** — O(1) lookup, filters to verified users only
3. **Jurisdiction** — O(n) lookup (n = allowed jurisdictions), more expensive

This minimizes wasted computation if early checks fail.

## Future Enhancements

1. **Permit-style transfers** — Add a two-step transfer with approval (ERC-20 style)
2. **Burn function** — Allow token destruction (admin or public)
3. **Pause mechanism** — Allow admin to pause all transfers temporarily
4. **Configurable jurisdictions** — Store allowed jurisdictions in contract storage
5. **Event emission** — Emit transfer events for indexing and off-chain observation
6. **Decimal/symbol support** — Add standard token metadata
7. **Balance queries** — Public getters for token metadata
8. **Composable limits** — Add per-user transfer limits (another primitive)

## References

- [`/contracts/denylist-gate`](../../contracts/denylist-gate) — Denylist primitive
- [`/contracts/allowlist-token`](../../contracts/allowlist-token) — Allowlist primitive
- [`/contracts/jurisdiction-flag`](../../contracts/jurisdiction-flag) — Jurisdiction primitive
- [`/examples/denylist-gate-consumer`](../denylist-gate-consumer) — Simpler single-primitive example
