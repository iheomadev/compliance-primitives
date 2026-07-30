# Jurisdiction-Flag Contract Upgrade Mechanism

## Overview

The `jurisdiction-flag` contract implements Soroban's native contract upgrade mechanism via the `upgrade()` function. This allows the issuer to deploy bug fixes and improvements without losing existing jurisdiction mappings or requiring address migration.

## How It Works

### The Upgrade Function

```rust
pub fn upgrade(env: Env, issuer: Address, new_wasm: Bytes) -> Result<(), Error>
```

The upgrade function:
1. Requires issuer authentication (identical to `set_jurisdiction`)
2. Calls `env.deployer().update_current_contract_wasm(new_wasm)` to swap the running code
3. Emits an `UpgradePerformed` event for auditability
4. Does **not** modify any storage — all jurisdiction mappings and the issuer key remain intact

### Storage Compatibility

The upgrade mechanism relies on **storage layout stability**:

- **Instance storage** (the issuer key) is preserved automatically by Soroban
- **Persistent storage** (jurisdiction mappings keyed by `DataKey::Jurisdiction`) survives the upgrade unchanged

If a new version needs to migrate the storage schema (e.g., changing the `DataKey` enum), a migration function must be called after the upgrade. This is a manual step, not automatic.

## Security Model

### Who Can Upgrade?

Only the **issuer address** specified during `initialize()` can trigger an upgrade. This is enforced by `require_issuer()`, which:
1. Calls `issuer.require_auth()` — the Soroban SDK ensures the issuer has signed the transaction
2. Verifies the provided issuer matches the stored issuer key
3. Returns `Error::NotAuthorized` if either check fails

### Storage Guarantees

1. **Instance storage is preserved**: The issuer key cannot be corrupted or lost during an upgrade
2. **Persistent storage is preserved**: All jurisdiction mappings survive the upgrade as long as the storage keys remain the same
3. **No token loss risk**: Unlike token transfers, there is no value transfer during an upgrade, so no funds are at risk if an upgrade fails mid-execution

### Auditability

Every upgrade emits an `UpgradePerformed` event with the issuer's address. Auditors and on-chain observers can:
- Track all upgrades that have occurred
- Verify that only the authorized issuer triggered each upgrade
- Correlate upgrades with any subsequent behavior changes

### Rollback and Safety

Soroban does **not** have automatic rollback. If a new contract version has a critical bug:
1. The issuer must prepare a fixed version (version 3)
2. The issuer calls `upgrade()` again with the fixed wasm
3. The old (buggy) version is abandoned — only the latest deployed code runs

This is why testing the upgrade path thoroughly before mainnet deployment is critical.

## Threat Model

### If the Issuer Key is Compromised

An attacker with the issuer's private key can:
- Call `upgrade()` with malicious contract code
- Steal all funds transferred to the contract (if it were a token)
- Change jurisdiction codes for any address
- Disable the contract entirely

**Mitigation**: Keep the issuer key in secure cold storage or a multisig wallet.

### If the Deployer Host Function is Buggy

If Soroban's `update_current_contract_wasm` has a vulnerability, an attacker might:
- Corrupt existing storage
- Bypass authentication

**Mitigation**: Rely on Stellar/SDF's audits and use on-chain observation to catch unexpected behavior.

### If the Upgrade Code is Buggy

If the new contract version has a bug:
- Users are blocked/allowed incorrectly
- Jurisdictions are corrupted or lost

**Mitigation**: Test thoroughly in testnet before mainnet. Simulate the upgrade process with a copy of the contract and existing storage.

## Testing the Upgrade Path

### Test Scenario: Add New Jurisdiction, Upgrade, Verify Survival

1. Initialize the contract
2. Set a jurisdiction mapping (e.g., Alice → "US")
3. Call `upgrade()` with new wasm
4. Query the jurisdiction mapping — it should still be "US"
5. Verify the issuer can still call `set_jurisdiction()` on the new version
6. Verify the new version handles storage the same way

### Test Scenario: Non-Issuer Cannot Upgrade

1. Initialize the contract
2. Have a non-issuer call `upgrade()` — should return `Error::NotAuthorized`
3. Verify the contract was not actually upgraded (can be inferred by issuer-only functions still working)

## Comparison to Other Chains

**Ethereum/EVM**: Typically use the Proxy pattern (a separate proxy contract forwards calls to an implementation contract). Storage is in the proxy, implementation can be swapped.

**Soroban**: Uses direct contract code replacement. Storage layout must remain compatible, or a migration is needed.

## Future Enhancements

1. **Storage migration helper**: Add a dedicated `migrate_storage()` function if the schema ever needs to change
2. **Version tracking**: Store the contract version in storage to help with future migrations
3. **Staged upgrades**: A multi-sig issuer could gate upgrades behind governance votes before calling `upgrade()`

## References

- Soroban Host Function: `update_current_contract_wasm` ([Soroban SDK docs](https://docs.rs/soroban-sdk/latest/soroban_sdk/struct.Deployer.html#method.update_current_contract_wasm))
- Storage Model: [Soroban Storage](https://soroban.stellar.org/docs/learn/storing-data)
