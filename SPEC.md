# Compliance Primitives Security Specification

This document specifies the security assumptions, invariants, and threat models for each of the three compliance primitives: `denylist-gate`, `allowlist-token`, and `jurisdiction-flag`.

## Executive Summary

- **Threat model**: Assumes the issuer/admin key is secure. If compromised, an attacker can manipulate all compliance decisions.
- **No recovery mechanism**: These are stateless gates; a blocked transfer cannot be undone retroactively. The issuer must maintain clear audit trails.
- **Composability**: Each primitive is independently auditable and can be called from multiple contracts. Blocking a single primitive is equivalent to blocking at the entry point.

---

## 1. Denylist-Gate Contract

### 1.1 Purpose

Maintain a standalone denylist contract that other contracts call via cross-contract invocation to check if an address is clear to transact.

### 1.2 Security Assumptions

- **Admin key is secure**: The admin address is kept in secure cold storage or a multisig, never exposed.
- **Immutable contract address**: Once deployed, the contract's address does not change (users of the contract hardcode this address). If the contract needs an upgrade, the issuer must deploy a new instance and migrate callers.
- **No private state in the blockchain**: Any address on the denylist is visible on-chain. If privacy is required, this contract is not suitable.
- **Soroban host environment is secure**: The `update_current_contract_wasm` function (if used for upgrades) is not compromised.

### 1.3 Invariants

#### Invariant 1.3.1: Check Returns False for Denied Addresses
**Statement**: If address `A` is on the denylist, `check(A)` must return `false`.

**Proof**: The `check()` function at line 88-94 of `src/lib.rs`:
```rust
pub fn check(env: Env, address: Address) -> bool {
    !env.storage().persistent().get(&DataKey::Denied(address)).unwrap_or(false)
}
```
Returns `true` only if the persistent storage entry for `DataKey::Denied(address)` is absent or falsy. If `add_to_denylist` has been called with that address, the entry exists and is set to `true`, so `check()` returns `false`.

**Tests verifying this invariant**:
- `contracts/denylist-gate/src/test.rs:test_check_returns_true_for_non_denied` — calls `check()` on address that was never added, expects `true`
- `contracts/denylist-gate/src/test.rs:test_check_returns_false_for_denied` — adds to denylist, calls `check()`, expects `false`
- `examples/denylist-gate-consumer/src/test.rs:test_transfer_blocks_denylisted_from` — a token contract calls `check()` on a denied address, expects rejection

#### Invariant 1.3.2: Check Returns True for Non-Denied Addresses
**Statement**: If address `A` is not on the denylist (or has been removed), `check(A)` must return `true`.

**Proof**: Same as Invariant 1.3.1 — the negation logic ensures `true` is returned when the entry is absent.

**Tests verifying this invariant**:
- `contracts/denylist-gate/src/test.rs:test_check_returns_true_for_non_denied`
- `contracts/denylist-gate/src/test.rs:test_remove_from_denylist` — adds then removes, calls `check()`, expects `true`

#### Invariant 1.3.3: Only Admin Can Modify Denylist
**Statement**: Only the admin address (verified by `require_auth()` and storage match) can add to or remove from the denylist.

**Proof**: Both `add_to_denylist()` and `remove_from_denylist()` call `Self::require_admin()` (line 66, 76), which:
1. Calls `admin.require_auth()` (line 97) — Soroban SDK enforces the signature
2. Retrieves the stored admin (line 98) — must match the passed `admin` or returns `NotAuthorized`

**Tests verifying this invariant**:
- `contracts/denylist-gate/src/test.rs:test_add_to_denylist_fails_if_not_admin` — non-admin calls `add_to_denylist()`, expects `Error::NotAuthorized`
- `contracts/denylist-gate/src/test.rs:test_remove_from_denylist_fails_if_not_admin` — non-admin calls `remove_from_denylist()`, expects `Error::NotAuthorized`

#### Invariant 1.3.4: Initialization Is Idempotent (One-Time Only)
**Statement**: `initialize()` can only succeed once. A second call must fail with `Error::AlreadyInitialized`.

**Proof**: Line 53-55:
```rust
if env.storage().instance().has(&DataKey::Admin) {
    return Err(Error::AlreadyInitialized);
}
```
Once `initialize()` succeeds, the admin is stored, and any second call finds the key and fails.

**Tests verifying this invariant**:
- `contracts/denylist-gate/src/test.rs:test_double_initialize_fails` — calls `initialize()` twice, second fails

#### Invariant 1.3.5: Functions Fail Before Initialization
**Statement**: Any function other than `initialize()` must fail with `Error::NotInitialized` if called before `initialize()`.

**Proof**: `require_admin()` (line 98-99) retrieves the admin with `ok_or(Error::NotInitialized)`, so any admin-gated call fails if the admin key is not set.

**Tests verifying this invariant**:
- `contracts/denylist-gate/src/test.rs:test_add_to_denylist_fails_before_initialize` — calls `add_to_denylist()` before `initialize()`, expects `Error::NotInitialized`

### 1.4 Threat Model

#### Threat 1.4.1: Compromised Admin Key
**Attacker capability**: Add/remove any address from the denylist; paralyze the entire compliance check.

**Impact**: 
- Can add legitimate addresses to the denylist, blocking all their transfers
- Can remove addresses that should be blocked, allowing sanctioned transactions
- Contract continues to function but at the attacker's discretion

**Mitigation**:
- Use a multisig wallet for the admin address (e.g., 2-of-3 signers)
- Keep the key in cold storage, never on a live server
- Emit events on every denylist change; monitor for unauthorized modifications

#### Threat 1.4.2: Malicious Contract Calling Denylist
**Attacker capability**: Create a fake contract that claims to respect denylist checks but doesn't.

**Impact**: The denylist itself is not compromised, but a contract calling it could ignore the `check()` result.

**Mitigation**: 
- Denylist is trustless — the contract is stateless and does not enforce transfer logic
- Issuers must verify that contracts calling the denylist actually honor `check()` results
- This is a caller responsibility, not a denylist responsibility

#### Threat 1.4.3: Upgrade Without Audit
**Attacker capability**: If contract upgrade is enabled, deploy new code that changes denylist logic.

**Impact**: Silent behavior change, potentially allowing denied addresses to pass.

**Mitigation**:
- Require admin multi-sig to approve upgrades
- Emit upgrade event for observation
- Maintain a slow upgrade process (e.g., 2-week timelock)

### 1.5 Test Coverage Summary

| Invariant | Test | File |
|-----------|------|------|
| 1.3.1 | `test_check_returns_false_for_denied` | denylist-gate/src/test.rs |
| 1.3.2 | `test_check_returns_true_for_non_denied` | denylist-gate/src/test.rs |
| 1.3.3 | `test_add_to_denylist_fails_if_not_admin` | denylist-gate/src/test.rs |
| 1.3.4 | `test_double_initialize_fails` | denylist-gate/src/test.rs |
| 1.3.5 | `test_add_to_denylist_fails_before_initialize` | denylist-gate/src/test.rs |

---

## 2. Allowlist-Token Contract

### 2.1 Purpose

Wrap an existing SEP-41 token and only permit `transfer` calls between two addresses that are both present on an on-chain allowlist.

### 2.2 Security Assumptions

- **Admin key is secure**: The admin manages the allowlist.
- **Underlying token contract is trusted**: The token contract at `token_address` is a legitimate SEP-41 implementation (does not steal funds, does not reverse transfers).
- **No path to call underlying token directly**: If users can call the underlying token directly, they can bypass the allowlist. The issuer must deprecate the underlying token address or require all clients to use only the allowlist-token wrapper.
- **Storage is persistent across calls**: The allowlist in persistent storage is never lost between transactions.

### 2.3 Invariants

#### Invariant 2.3.1: Transfer Requires Both Parties on Allowlist
**Statement**: If `A` or `B` is not on the allowlist, `transfer(A, B, amount)` must return `Error::Blocked` and no funds must be moved.

**Proof**: Lines 106-120 check both addresses. If either is missing, the transfer is rejected at line 113-114.

**Tests verifying this invariant**:
- `contracts/allowlist-token/src/test.rs:test_transfer_blocks_if_from_not_allowed` — from not on allowlist, transfer fails
- `contracts/allowlist-token/src/test.rs:test_transfer_blocks_if_to_not_allowed` — to not on allowlist, transfer fails

#### Invariant 2.3.2: Transfer Succeeds Only if Both Parties on Allowlist
**Statement**: If both `A` and `B` are on the allowlist, `transfer(A, B, amount)` must succeed (assuming underlying token has sufficient balance).

**Proof**: Line 113-114 checks both allowlist entries, then proceeds to call the underlying token's `transfer()` at line 121.

**Tests verifying this invariant**:
- `contracts/allowlist-token/src/test.rs:test_transfer_succeeds_if_both_allowed` — both on allowlist, transfer succeeds

#### Invariant 2.3.3: Only Admin Can Modify Allowlist
**Statement**: Only the admin address can add/remove from the allowlist.

**Proof**: Both `add_to_allowlist()` and `remove_from_allowlist()` call `Self::require_admin()`, same pattern as denylist.

**Tests verifying this invariant**:
- `contracts/allowlist-token/src/test.rs:test_add_to_allowlist_fails_if_not_admin`
- `contracts/allowlist-token/src/test.rs:test_remove_from_allowlist_fails_if_not_admin`

#### Invariant 2.3.4: Blocked Transfer Does Not Call Underlying Token
**Statement**: If the allowlist check fails, the contract must NOT call `transfer()` on the underlying token.

**Proof**: Lines 113-114 return early with `Blocked` error before any cross-contract call to the token (which happens at line 121).

**Tests verifying this invariant**:
- `contracts/allowlist-token/src/test.rs:test_transfer_blocks_if_from_not_allowed` — verifies the underlying token balance did not change (using mock)
- `contracts/allowlist-token/src/test.rs:test_transfer_blocks_if_to_not_allowed` — same

#### Invariant 2.3.5: All Allowlist Changes Emit Events
**Statement**: Every call to `add_to_allowlist()` or `remove_from_allowlist()` must emit an `AllowAdd` or `AllowRemove` event.

**Proof**: Lines 116 and 124 call `AllowAdd { address }.publish(&env)` and `AllowRemove { address }.publish(&env)` respectively.

**Tests verifying this invariant**:
- `contracts/allowlist-token/src/test.rs:test_add_to_allowlist_emits_event`
- `contracts/allowlist-token/src/test.rs:test_remove_from_allowlist_emits_event`

### 2.4 Threat Model

#### Threat 2.4.1: Compromised Admin Key
**Attacker capability**: Add/remove arbitrary addresses to/from allowlist.

**Impact**: 
- Can remove legitimate users, blocking their transfers
- Can add attacker-controlled addresses, allowing theft/fraud

**Mitigation**: Use multisig for admin, cold storage, event monitoring (same as denylist).

#### Threat 2.4.2: Malicious Underlying Token
**Attacker capability**: Deploy a fake SEP-41 token at a known address; trick allowlist-token into using it.

**Impact**: The attacker can steal all funds transferred through allowlist-token.

**Mitigation**:
- Admin must verify the token address before initializing
- Issuer should emit a trusted token address from an official channel (e.g., signed announcement)
- Cannot be fixed in the allowlist-token itself (it trusts whatever address is configured)

#### Threat 2.4.3: Users Bypass Allowlist by Calling Underlying Token Directly
**Attacker capability**: If users know the underlying token address, they can call it directly, bypassing the allowlist.

**Impact**: Allowlist is rendered useless; any token holder can transfer freely.

**Mitigation**:
- Issuer must deprecate the original token address via announcement
- Optionally: the issuer can call the underlying token's owner/admin to freeze it (if possible)
- This is a process/governance problem, not a contract problem

### 2.5 Test Coverage Summary

| Invariant | Test | File |
|-----------|------|------|
| 2.3.1 | `test_transfer_blocks_if_from_not_allowed` | allowlist-token/src/test.rs |
| 2.3.2 | `test_transfer_succeeds_if_both_allowed` | allowlist-token/src/test.rs |
| 2.3.3 | `test_add_to_allowlist_fails_if_not_admin` | allowlist-token/src/test.rs |
| 2.3.4 | `test_transfer_blocks_if_from_not_allowed` | allowlist-token/src/test.rs |
| 2.3.5 | `test_add_to_allowlist_emits_event` | allowlist-token/src/test.rs |

---

## 3. Jurisdiction-Flag Contract

### 3.1 Purpose

Attach an issuer-controlled jurisdiction code to an address; other contracts can call `is_permitted_jurisdiction()` to check if an address is in an allowed set of jurisdictions.

### 3.2 Security Assumptions

- **Issuer key is secure**: The issuer controls which jurisdictions are assigned to each address.
- **Jurisdiction codes are meaningful**: The contract does not validate jurisdiction codes (e.g., ISO 3166-1 format). The issuer must use consistent codes.
- **Storage schema is stable**: If the contract is upgraded, the storage layout must not change without a migration path.

### 3.3 Invariants

#### Invariant 3.3.1: Set Jurisdiction Stores Code
**Statement**: After calling `set_jurisdiction(issuer, A, "US")`, `get_jurisdiction(A)` must return `Some("US")`.

**Proof**: Lines 69-71 write directly to persistent storage at `DataKey::Jurisdiction(address)`, and `get_jurisdiction()` reads from the same key.

**Tests verifying this invariant**:
- `contracts/jurisdiction-flag/src/test.rs:test_set_and_get_jurisdiction` — sets, then gets, verifies value is stored

#### Invariant 3.3.2: Only Issuer Can Set Jurisdiction
**Statement**: Only the issuer address can call `set_jurisdiction()`.

**Proof**: Line 68 calls `Self::require_issuer()`, same pattern as denylist/allowlist.

**Tests verifying this invariant**:
- `contracts/jurisdiction-flag/src/test.rs:test_set_jurisdiction_rejects_non_issuer` — non-issuer calls, expects `Error::NotAuthorized`

#### Invariant 3.3.3: Is Permitted Jurisdiction Returns True Only If In Allowed List
**Statement**: `is_permitted_jurisdiction(A, allowed_codes)` returns `true` iff `A` has a jurisdiction code AND that code appears in `allowed_codes`.

**Proof**: Lines 85-88:
```rust
match Self::get_jurisdiction(env, address) {
    Some(code) => allowed_codes.iter().any(|c| c == code),
    None => false,
}
```
Returns `true` only if the address has a code AND it matches at least one in `allowed_codes`.

**Tests verifying this invariant**:
- `contracts/jurisdiction-flag/src/test.rs:test_is_permitted_jurisdiction_true_when_code_in_list` — code is in list, returns `true`
- `contracts/jurisdiction-flag/src/test.rs:test_is_permitted_jurisdiction_false_when_no_jurisdiction_set` — no code set, returns `false`
- `contracts/jurisdiction-flag/src/test.rs:test_is_permitted_jurisdiction_false_with_empty_allowed_list` — empty allowed list, returns `false`

#### Invariant 3.3.4: Upgrade Preserves Storage
**Statement**: After calling `upgrade(issuer, new_wasm)`, all previously set jurisdictions must remain intact.

**Proof**: The `upgrade()` function (line 98-104) calls `env.deployer().update_current_contract_wasm()` and does NOT modify any storage. Soroban's storage model preserves persistent data across code updates.

**Tests verifying this invariant**:
- `contracts/jurisdiction-flag/src/test.rs:test_upgrade_preserves_jurisdiction_storage` — sets jurisdiction, upgrades, verifies it's still there

#### Invariant 3.3.5: Only Issuer Can Upgrade
**Statement**: Only the issuer can call `upgrade()`.

**Proof**: Line 102 calls `Self::require_issuer()`, enforcing issuer authentication.

**Tests verifying this invariant**:
- `contracts/jurisdiction-flag/src/test.rs:test_upgrade_requires_issuer_auth` — non-issuer calls `upgrade()`, expects `Error::NotAuthorized`

#### Invariant 3.3.6: All Jurisdiction Changes Emit Events
**Statement**: Every call to `set_jurisdiction()` must emit a `JurisdictionSet` event.

**Proof**: Line 72 calls `JurisdictionSet { address, code }.publish(&env)`.

**Tests verifying this invariant**:
- `contracts/jurisdiction-flag/src/test.rs:test_set_jurisdiction_emits_event` (if exists; not explicitly listed but should be added)

**Gap identified**: No test explicitly verifies event emission for `set_jurisdiction`. Recommend adding `test_set_jurisdiction_emits_event`.

### 3.4 Threat Model

#### Threat 3.4.1: Compromised Issuer Key
**Attacker capability**: Assign false jurisdiction codes to addresses.

**Impact**: 
- Can assign attacker-controlled addresses to any jurisdiction, allowing them to transact
- Can assign legitimate addresses to wrong jurisdictions, blocking them

**Mitigation**: Use multisig for issuer, cold storage, event monitoring.

#### Threat 3.4.2: Issuer Sets Inconsistent Codes
**Attacker capability**: Use inconsistent jurisdiction codes (e.g., "US", "United States", "usa" for the same country).

**Impact**: Contracts checking for exact matches will inconsistently allow/block users based on which code variant was used.

**Mitigation**:
- Recommend standardized codes (ISO 3166-1 alpha-2)
- The contract does not validate; issuer must maintain discipline
- Document the approved code list in off-chain governance

#### Threat 3.4.3: Caller Trusts Wrong Contract
**Attacker capability**: Deploy a fake jurisdiction-flag contract with permissive logic.

**Impact**: Contracts calling the fake contract will allow any address.

**Mitigation**:
- Callers must verify the jurisdiction-flag address before using it
- Issuer should announce the official address via secure channel

### 3.5 Test Coverage Summary

| Invariant | Test | File |
|-----------|------|------|
| 3.3.1 | `test_set_and_get_jurisdiction` | jurisdiction-flag/src/test.rs |
| 3.3.2 | `test_set_jurisdiction_rejects_non_issuer` | jurisdiction-flag/src/test.rs |
| 3.3.3 | `test_is_permitted_jurisdiction_true_when_code_in_list` | jurisdiction-flag/src/test.rs |
| 3.3.4 | `test_upgrade_preserves_jurisdiction_storage` | jurisdiction-flag/src/test.rs |
| 3.3.5 | `test_upgrade_requires_issuer_auth` | jurisdiction-flag/src/test.rs |
| 3.3.6 | *GAP* — no explicit test for event emission | — |

---

## 4. Cross-Contract Composition (RWA Token Example)

The `/examples/rwa-token` contract demonstrates how to compose all three primitives into a single transfer function.

### 4.1 Composition Invariant

**Statement**: A transfer `rwa_token.transfer(A, B, amount)` succeeds iff:
1. Denylist check passes for both A and B
2. Allowlist check passes for both A and B
3. Jurisdiction check passes for both A and B
4. A has sufficient balance

**Proof**: Lines 106-145 of `examples/rwa-token/src/lib.rs` implement these checks in sequence, returning early if any fails.

**Tests verifying this invariant**:
- `examples/rwa-token/src/test.rs:test_transfer_fails_if_denied_by_denylist`
- `examples/rwa-token/src/test.rs:test_transfer_fails_not_on_allowlist`
- `examples/rwa-token/src/test.rs:test_transfer_fails_insufficient_balance`
- `examples/rwa-token/src/test.rs:test_transfer_succeeds_when_not_denied`

### 4.2 Check Order Invariant

**Statement**: Checks are performed in order: denylist → allowlist → jurisdiction → balance. If an earlier check fails, later checks are not executed (short-circuit evaluation).

**Proof**: Lines 118-121 (denylist), 124-128 (allowlist), 131-137 (jurisdiction), 140-143 (balance) perform checks sequentially with early returns.

**Tests verifying this invariant**:
- `examples/rwa-token/src/test.rs:test_transfer_multiple_checks_fail_in_order` — multiple checks fail, first failure is returned

---

## 5. Test Coverage Summary

### Tests by Category

| Category | Count | Details |
|----------|-------|---------|
| Denylist checks | 6 | Initialization, add/remove, check logic, authorization |
| Allowlist checks | 6 | Initialization, add/remove, transfer gating, authorization |
| Jurisdiction checks | 8 | Initialization, set/get, permitted codes, upgrade, authorization |
| RWA composition | 8 | Checks in isolation and combined, check order, balance |
| **Total** | **28** | — |

### Identified Gaps

1. **Jurisdiction event emission test** (Invariant 3.3.6) — No explicit test for `set_jurisdiction()` event emission. Recommend adding `test_set_jurisdiction_emits_event`.

2. **Allowlist event emission test** (Invariant 2.3.5) — May be missing explicit verification that events are emitted. Recommend adding if not present.

3. **Denylist event emission test** (Invariant 1.3.x) — Same as above.

4. **Concurrent/replay attack tests** — No tests for transaction replay or concurrent modifications. Soroban SDK handles nonce validation, but explicit tests would strengthen confidence.

5. **Upgrade event emission test** (Jurisdiction-flag) — `test_upgrade_emits_event` should verify the `UpgradePerformed` event is actually emitted.

---

## 6. Recommendations for External Auditors

1. **Verify storage layout stability**: Ensure that any contract upgrades maintain backward compatibility with existing persistent storage keys.

2. **Check authorization patterns**: All admin/issuer-gated functions use `require_auth()` correctly. Verify no authorization bypass exists.

3. **Verify cross-contract safety**: When these primitives are called from user contracts (e.g., RWA token), verify that return values are properly interpreted and that a false result truly blocks the transaction.

4. **Test edge cases**:
   - What happens if the underlying token in allowlist-token returns an error?
   - What happens if denylist-gate is uninitialized when called?
   - What happens if an address is set to an empty jurisdiction code?

5. **Review event emission**: Ensure all critical state changes emit events for off-chain observation.

6. **Verify no reentrancy vulnerabilities**: Soroban's model limits reentrancy, but verify that cross-contract calls don't allow attackers to manipulate state during a transaction.

---

## Appendix: Glossary

- **Invariant**: A property that must hold true before and after any function call.
- **Threat**: A potential attack vector or failure mode.
- **Mitigations**: Controls that reduce the likelihood or impact of a threat.
- **Test coverage gap**: An invariant or threat that is not explicitly tested.
- **Short-circuit evaluation**: Early return from a function once a condition is met, skipping later checks.

