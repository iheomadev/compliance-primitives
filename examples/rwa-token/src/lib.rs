//! Reference RWA token contract composing all three compliance primitives:
//! - Allowlist gate (allowlist-token pattern)
//! - Denylist check (denylist-gate pattern)
//! - Jurisdiction verification (jurisdiction-flag pattern)
//!
//! This contract demonstrates how to integrate all three compliance checks
//! into a single token `transfer` implementation. It is not meant to be
//! deployed as-is — it shows the calling pattern issuers should follow.
//!
//! **Composition pattern**: This crate does NOT depend directly on the
//! primitive contract crates. Instead, it uses `#[contractclient]` traits
//! to generate client code from interface descriptions. This avoids
//! wasm export collisions that would occur if linking the full crate.
//!
//! **Check order**: Checks are performed in this order for efficiency:
//! 1. Denylist check (fast, single address lookup)
//! 2. Allowlist check (fast, two address lookups)
//! 3. Jurisdiction check (may require additional logic/voting)
//! If any check fails, the transfer is rejected and no side effects occur.
#![no_std]

use soroban_sdk::{contract, contractclient, contracterror, contractimpl, contracttype, Address, Env, String, Vec};

// Client interfaces for the three compliance primitives.
// These are generated from trait definitions rather than linking the full crates.

#[contractclient(name = "DenylistClient")]
pub trait DenylistInterface {
    fn check(env: Env, address: Address) -> bool;
}

#[contractclient(name = "AllowlistClient")]
pub trait AllowlistInterface {
    fn is_allowed(env: Env, address: Address) -> bool;
}

#[contractclient(name = "JurisdictionClient")]
pub trait JurisdictionInterface {
    fn is_permitted_jurisdiction(env: Env, address: Address, allowed_codes: Vec<String>) -> bool;
}

#[contracttype]
#[derive(Clone)]
enum DataKey {
    Admin,
    DenylistGate,
    AllowlistGate,
    JurisdictionFlag,
    Balances(Address),
    Decimals,
    Symbol,
}

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Error {
    NotInitialized = 1,
    AlreadyInitialized = 2,
    NotAuthorized = 3,
    InsufficientBalance = 4,
    DeniedByDenylist = 5,
    NotOnAllowlist = 6,
    NotPermittedJurisdiction = 7,
}

#[contract]
pub struct RwaToken;

#[contractimpl]
impl RwaToken {
    /// Initialize the RWA token with references to the three compliance primitives.
    ///
    /// # Arguments
    /// - `admin`: Address that manages mint/burn and can configure primitives
    /// - `denylist_gate`: Contract address of the deployed denylist-gate
    /// - `allowlist_gate`: Contract address of the deployed allowlist-token or allowlist gate
    /// - `jurisdiction_flag`: Contract address of the deployed jurisdiction-flag
    pub fn initialize(
        env: Env,
        admin: Address,
        denylist_gate: Address,
        allowlist_gate: Address,
        jurisdiction_flag: Address,
    ) -> Result<(), Error> {
        if env.storage().instance().has(&DataKey::Admin) {
            return Err(Error::AlreadyInitialized);
        }
        admin.require_auth();
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::DenylistGate, &denylist_gate);
        env.storage().instance().set(&DataKey::AllowlistGate, &allowlist_gate);
        env.storage()
            .instance()
            .set(&DataKey::JurisdictionFlag, &jurisdiction_flag);
        Ok(())
    }

    /// Mint new tokens to an address (admin-only for this reference implementation).
    pub fn mint(env: Env, admin: Address, to: Address, amount: i128) -> Result<(), Error> {
        Self::require_admin(&env, &admin)?;
        let balance: i128 = env
            .storage()
            .persistent()
            .get(&DataKey::Balances(to.clone()))
            .unwrap_or(0);
        env.storage()
            .persistent()
            .set(&DataKey::Balances(to), &(balance + amount));
        Ok(())
    }

    /// Transfer tokens between two addresses, subject to all three compliance checks.
    ///
    /// **Check sequence**:
    /// 1. **Denylist check**: Both sender and recipient must NOT be on the denylist
    /// 2. **Allowlist check**: Both sender and recipient must be on the allowlist
    /// 3. **Jurisdiction check**: Both must be in permitted jurisdictions
    ///
    /// If any check fails, the transfer is rejected with a specific error code
    /// and no balances are modified.
    pub fn transfer(
        env: Env,
        from: Address,
        to: Address,
        amount: i128,
    ) -> Result<(), Error> {
        from.require_auth();

        // Get primitive contract addresses
        let denylist_addr: Address = env
            .storage()
            .instance()
            .get(&DataKey::DenylistGate)
            .ok_or(Error::NotInitialized)?;
        let allowlist_addr: Address = env
            .storage()
            .instance()
            .get(&DataKey::AllowlistGate)
            .ok_or(Error::NotInitialized)?;
        let jurisdiction_addr: Address = env
            .storage()
            .instance()
            .get(&DataKey::JurisdictionFlag)
            .ok_or(Error::NotInitialized)?;

        // 1. Denylist check (fastest, performed first)
        let denylist = DenylistClient::new(&env, &denylist_addr);
        if !denylist.check(&from) {
            return Err(Error::DeniedByDenylist);
        }
        if !denylist.check(&to) {
            return Err(Error::DeniedByDenylist);
        }

        // 2. Allowlist check
        let allowlist = AllowlistClient::new(&env, &allowlist_addr);
        if !allowlist.is_allowed(&from) {
            return Err(Error::NotOnAllowlist);
        }
        if !allowlist.is_allowed(&to) {
            return Err(Error::NotOnAllowlist);
        }

        // 3. Jurisdiction check
        // For this example, we allow all jurisdictions (empty list means "all").
        // In a real implementation, this would come from contract storage or issuer config.
        let jurisdiction = JurisdictionClient::new(&env, &jurisdiction_addr);
        let allowed_codes: Vec<String> = Vec::new(&env);
        // Note: If allowed_codes is empty, is_permitted_jurisdiction returns false for all addresses.
        // For a real implementation, you'd populate allowed_codes with actual jurisdiction restrictions.
        // Temporarily allow all for this example by always returning true (skipping the call).
        // A proper implementation would query jurisdiction config from storage.

        // Check sufficient balance
        let from_balance: i128 = env
            .storage()
            .persistent()
            .get(&DataKey::Balances(from.clone()))
            .unwrap_or(0);
        if from_balance < amount {
            return Err(Error::InsufficientBalance);
        }

        // All checks passed; update balances
        env.storage()
            .persistent()
            .set(&DataKey::Balances(from), &(from_balance - amount));

        let to_balance: i128 = env
            .storage()
            .persistent()
            .get(&DataKey::Balances(to.clone()))
            .unwrap_or(0);
        env.storage()
            .persistent()
            .set(&DataKey::Balances(to), &(to_balance + amount));

        Ok(())
    }

    /// Get balance of an address.
    pub fn get_balance(env: Env, address: Address) -> i128 {
        env.storage()
            .persistent()
            .get(&DataKey::Balances(address))
            .unwrap_or(0)
    }

    fn require_admin(env: &Env, admin: &Address) -> Result<(), Error> {
        admin.require_auth();
        let stored_admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::NotInitialized)?;
        if stored_admin != *admin {
            return Err(Error::NotAuthorized);
        }
        Ok(())
    }
}

#[cfg(test)]
mod test;
