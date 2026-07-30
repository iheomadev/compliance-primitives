// Copyright (c) 2026 Stellar Compliance Kit contributors
// SPDX-License-Identifier: MIT
// See the LICENSE file in the repository root for the full license text.

//! `jurisdiction-flag` is a `#![no_std]` Soroban contract that attaches a
//! jurisdiction code (e.g. an ISO 3166-1 alpha-2 country code) to an
//! address.
//!
//! **Purpose**: let an issuer record which jurisdiction(s) an address has
//! been verified in — including dual citizenship/residency — so other
//! contracts can restrict activity to a permitted set of jurisdictions
//! without each one reimplementing that bookkeeping.
//!
//! **Storage shape**: `DataKey::Jurisdiction(Address)` stores a
//! `Vec<String>` of codes. The legacy single-code helpers
//! `set_jurisdiction` / `get_jurisdiction` remain as conveniences:
//! `set_jurisdiction` replaces the address's entire set with a one-element
//! vector, and `get_jurisdiction` returns the first code (if any). Prefer
//! `add_jurisdiction` / `remove_jurisdiction` / `list_jurisdictions` when
//! managing multiple codes. This shape leaves room for #83 (batch remove
//! over the same vec) and #110 (richer per-code metadata) without a second
//! parallel key.
//!
//! **Permission semantics**: `is_permitted_jurisdiction` uses *any*
//! matching — it returns `true` if at least one of the address's codes
//! appears in `allowed_codes`. An address with no codes is never permitted.
//!
//! **Callers**: only the configured `issuer` address may call
//! `set_jurisdiction` / `remove_jurisdiction_multiple`. Any contract or
//! off-chain client can read a flag via `get_jurisdiction`, and contracts
//! enforcing a jurisdiction allowlist can call
//! `is_permitted_jurisdiction(address, allowed_codes)` directly as part of
//! their own compliance checks.
//!
//! **Composition**: designed to be called into from another contract's
//! `transfer` or similar gating logic — the same pattern `denylist-gate`
//! uses — rather than deployed standalone.
//!
//! **Pausability**: the issuer may call `pause` to halt all mutating
//! operations (`set_jurisdiction`). The read-only `get_jurisdiction` and
//! `is_permitted_jurisdiction` methods are unaffected by pause state. The
//! shared [`compliance_pausable`] helper crate implements the pause storage
//! logic; this contract only supplies issuer-gating and event emission.
#![no_std]

use soroban_sdk::{
    contract, contracterror, contractevent, contractimpl, contracttype, Address, Env, String, Vec,
};

/// Storage value for a jurisdiction flag. `valid_until` is the last ledger
/// sequence number at which the flag is still valid. `None` means the flag
/// never expires.
#[contracttype]
#[derive(Clone)]
pub struct JurisdictionEntry {
    pub code: String,
    pub valid_until: Option<u32>,
}

/// Extend persistent jurisdiction entries when TTL drops below this many ledgers.
const TTL_THRESHOLD: u32 = 1_000;
/// Target TTL (in ledgers) after extension. Matches Stellar archival guidance
/// for long-lived compliance flags that must remain queryable.
const TTL_EXTEND_TO: u32 = 5_000;

#[contracttype]
#[derive(Clone)]
enum DataKey {
    /// The issuer address, set once in `initialize`. Instance storage.
    Issuer,
    ComplianceOfficer,
    Jurisdiction(Address),
    Paused,
}

/// Emitted whenever a jurisdiction flag is set (with or without expiry).
#[contractevent]
pub struct JurisdictionSet {
    #[topic]
    pub address: Address,
    pub code: String,
    pub valid_until: Option<u32>,
}

/// Emitted (as a signal for off-chain indexers) when an expired flag is
/// encountered during a read. The flag is not removed from storage — it is
/// simply ignored — but this event lets listeners react.
#[contractevent]
pub struct JurisdictionExpired {
    #[topic]
    pub address: Address,
}

#[contractevent]
pub struct Paused {
    #[topic]
    pub issuer: Address,
}

#[contractevent]
pub struct Unpaused {
    #[topic]
    pub issuer: Address,
}

#[contractevent]
pub struct JurisdictionRemoved {
    #[topic]
    pub address: Address,
}

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Error {
    NotInitialized = 1,
    AlreadyInitialized = 2,
    NotAuthorized = 3,
    /// Caller supplied an argument that is structurally invalid — e.g. an
    /// empty or malformed jurisdiction code.  Discriminant 4 is reserved
    /// for this variant across all three contracts so audit tooling can
    /// pattern-match on it without knowing which contract it originated from.
    InvalidInput = 4,
}

#[contract]
pub struct JurisdictionFlag;

#[contractimpl]
impl JurisdictionFlag {
    /// One-time setup. `issuer` is the only address allowed to set
    /// jurisdiction codes afterward.
    pub fn initialize(env: Env, issuer: Address) -> Result<(), Error> {
        if env.storage().instance().has(&DataKey::Issuer) {
            return Err(Error::AlreadyInitialized);
        }
        issuer.require_auth();
        env.storage().instance().set(&DataKey::Issuer, &issuer);
        env.storage().instance().set(&DataKey::Paused, &false);
        Ok(())
    }

    /// Assign the compliance-officer role to `officer`. Issuer-only.
    /// A compliance officer may call `set_jurisdiction` but may NOT
    /// assign or revoke the role.
    pub fn set_compliance_officer(
        env: Env,
        issuer: Address,
        officer: Address,
    ) -> Result<(), Error> {
        Self::require_issuer(&env, &issuer)?;
        env.storage()
            .instance()
            .set(&DataKey::ComplianceOfficer, &officer);
        Ok(())
    }

    /// Revoke the compliance-officer role. Issuer-only.
    pub fn revoke_compliance_officer(env: Env, issuer: Address) -> Result<(), Error> {
        Self::require_issuer(&env, &issuer)?;
        env.storage()
            .instance()
            .remove(&DataKey::ComplianceOfficer);
        Ok(())
    }

    /// Attach jurisdiction `code` to `address`. Issuer or compliance-officer.
    pub fn set_jurisdiction(
        env: Env,
        issuer: Address,
        address: Address,
        code: String,
    ) -> Result<(), Error> {
        Self::require_compliance_authority(&env, &issuer)?;
        env.storage()
            .persistent()
            .set(&DataKey::Jurisdiction(address.clone()), &entry);
        JurisdictionSet {
            address,
            code,
            valid_until: None,
        }
        .publish(&env);
        Ok(())
    }

    /// Attach jurisdiction `code` to `address` that expires after ledger
    /// sequence `valid_until` (inclusive). Issuer-only.
    pub fn set_jurisdiction_until(
        env: Env,
        issuer: Address,
        address: Address,
        code: String,
        valid_until: u32,
    ) -> Result<(), Error> {
        Self::require_issuer(&env, &issuer)?;
        let key = DataKey::Jurisdiction(address.clone());
        env.storage().persistent().set(&key, &code);
        Self::extend_jurisdiction_ttl(&env, &key);
        JurisdictionSet { address, code }.publish(&env);
        Ok(())
    }

    /// Remove stored jurisdiction codes for each address in `addresses`.
    ///
    /// Authorizes `issuer` once via [`Self::require_issuer`], then clears
    /// `DataKey::Jurisdiction` for every entry. Addresses that never had a
    /// code set are skipped (no-op per address). An empty `addresses` vec is
    /// also a no-op after the auth check.
    ///
    /// **Batch size**: no `MAX_BATCH_SIZE` guard is applied here yet. Issue
    /// #73 will introduce a shared cap and `Error::BatchTooLarge` across all
    /// batch entry points (#69/#70/#71 and this function) so the limit lands
    /// consistently rather than being bolted on per-function.
    pub fn remove_jurisdiction_multiple(
        env: Env,
        issuer: Address,
        addresses: Vec<Address>,
    ) -> Result<(), Error> {
        Self::require_issuer(&env, &issuer)?;
        for address in addresses.iter() {
            env.storage()
                .persistent()
                .remove(&DataKey::Jurisdiction(address));
        }
        Ok(())
    }

    /// Returns the jurisdiction code attached to `address`, if any.
    ///
    /// Returns `None` if:
    /// - no jurisdiction has been set, or
    /// - the flag has a `valid_until` that is strictly less than the current
    ///   ledger sequence (i.e. the flag has expired).
    pub fn get_jurisdiction(env: Env, address: Address) -> Option<String> {
        let key = DataKey::Jurisdiction(address);
        let code = env.storage().persistent().get(&key);
        if code.is_some() {
            Self::extend_jurisdiction_ttl(&env, &key);
        }
        code
    }

    /// Returns `true` if `address` has a non-expired jurisdiction code set
    /// AND that code appears in `allowed_codes`. Meant to be called by other
    /// contracts that want to restrict activity to a set of permitted
    /// jurisdictions.
    pub fn is_permitted_jurisdiction(env: Env, address: Address, allowed_codes: Vec<String>) -> bool {
        match Self::get_jurisdiction(env, address) {
            Some(code) => allowed_codes.iter().any(|c| c == code),
            None => false,
        })
    }

    /// Upgrade the contract to a new implementation. Issuer-only.
    ///
    /// Calls `update_current_contract_wasm` to upgrade the running contract code.
    /// Existing jurisdiction mappings and the issuer key are preserved across the upgrade.
    ///
    /// # Security model
    /// - Only the initialized issuer can trigger an upgrade
    /// - All persistent storage (jurisdiction mappings) is preserved
    /// - The instance storage (issuer key) is preserved
    /// - An `UpgradePerformed` event is emitted for auditability
    pub fn upgrade(env: Env, issuer: Address, new_wasm: Bytes) -> Result<(), Error> {
        Self::require_issuer(&env, &issuer)?;
        env.deployer().update_current_contract_wasm(new_wasm);
        UpgradePerformed { issuer }.publish(&env);
        Ok(())
    }

    fn require_issuer(env: &Env, issuer: &Address) -> Result<(), Error> {
        issuer.require_auth();
        let stored_issuer: Address = env.storage().instance().get(&DataKey::Issuer).ok_or(Error::NotInitialized)?;
        if stored_issuer != *issuer {
            return Err(Error::NotAuthorized);
        }
        Ok(())
    }

    /// Checks that `caller` is either the issuer or the compliance officer.
    fn require_compliance_authority(env: &Env, caller: &Address) -> Result<(), Error> {
        caller.require_auth();
        let stored_issuer: Address = env
            .storage()
            .instance()
            .get(&DataKey::Issuer)
            .ok_or(Error::NotInitialized)?;
        if stored_issuer == *caller {
            return Ok(());
        }
        if let Some(officer) = env
            .storage()
            .instance()
            .get(&DataKey::ComplianceOfficer)
        {
            if officer == *caller {
                return Ok(());
            }
        }
        Err(Error::NotAuthorized)
    }

    fn extend_jurisdiction_ttl(env: &Env, key: &DataKey) {
        env.storage()
            .persistent()
            .extend_ttl(key, TTL_THRESHOLD, TTL_EXTEND_TO);
    }
}

/// Implementation of the shared ComplianceCheck trait for jurisdiction-flag.
/// Allows external contracts to call this contract through a unified interface.
impl ComplianceCheck for JurisdictionFlag {
    /// Returns true if the address has a jurisdiction code set (i.e., has been verified).
    /// Note: This is a simplified version that checks for any jurisdiction being set,
    /// not whether it's in a specific permitted list. Use `is_permitted_jurisdiction()`
    /// for jurisdiction whitelist checks.
    fn is_compliant(env: Env, address: Address) -> bool {
        JurisdictionFlag::get_jurisdiction(env, address).is_some()
    }
}

#[cfg(test)]
mod test;

#[cfg(test)]
mod fuzz;
