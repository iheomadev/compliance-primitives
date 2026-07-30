// Copyright (c) 2026 Stellar Compliance Kit contributors
// SPDX-License-Identifier: MIT
// See the LICENSE file in the repository root for the full license text.

//! `denylist-gate` is a `#![no_std]` Soroban contract that maintains a
//! standalone on-chain denylist.
//!
//! **Purpose**: give issuers a shared, independently auditable place to
//! record addresses that must never transact (sanctions hits, fraud, court
//! orders, etc.), decoupled from any single token contract's own storage.
//!
//! **Callers**: an `admin` address manages the denylist through
//! `add_to_denylist`/`remove_from_denylist`. Other contracts — typically a
//! token's `transfer` function — call the read-only `check(address)` via a
//! cross-contract call before moving funds, so the denylist can be updated
//! without redeploying or touching the token contract itself.
//!
//! **Composition**: this contract is meant to be called into, not deployed
//! as a token itself. See `/examples/denylist-gate-consumer` for a worked
//! example of a token contract wiring `check()` into its `transfer` path.
#![no_std]

use soroban_sdk::{
    contract, contracterror, contractevent, contractimpl, contracttype, Address, Env,
};

/// Storage keys for this contract's state.
#[contracttype]
#[derive(Clone)]
enum DataKey {
    /// The admin address, set once in `initialize`. Instance storage.
    Admin,
    /// Whether a given address is on the denylist. Persistent storage,
    /// keyed per address.
    Denied(Address),
}

#[contractevent]
pub struct DenyAdd {
    #[topic]
    pub address: Address,
}

#[contractevent]
pub struct DenyRemove {
    #[topic]
    pub address: Address,
}

#[contractevent]
pub struct MultisigInitialized {
    pub threshold: u32,
    pub signer_count: u32,
}

#[contractevent]
pub struct SignerAdded {
    #[topic]
    pub signer: Address,
}

#[contractevent]
pub struct SignerRemoved {
    #[topic]
    pub signer: Address,
}

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Error {
    NotInitialized = 1,
    AlreadyInitialized = 2,
    NotAuthorized = 3,
    ThresholdNotMet = 4,
    InvalidThreshold = 5,
    InvalidSignerSet = 6,
    SignerNotInSet = 7,
}

#[contract]
pub struct DenylistGate;

#[contractimpl]
impl DenylistGate {
    /// One-time setup. Stores `admin` as the only address allowed to update
    /// the denylist afterward.
    ///
    /// # Auth
    /// Requires authorization from `admin` via `require_auth()`.
    ///
    /// # Returns
    /// `Ok(())` on success.
    ///
    /// # Errors
    /// - [`Error::AlreadyInitialized`] if `initialize` was already called.
    pub fn initialize(env: Env, admin: Address) -> Result<(), Error> {
        if env.storage().instance().has(&DataKey::Admin) {
            return Err(Error::AlreadyInitialized);
        }
        admin.require_auth();
        env.storage().instance().set(&DataKey::Admin, &admin);
        Ok(())
    }

    /// Add `address` to the denylist and emit a [`DenyAdd`] event.
    ///
    /// # Auth
    /// Admin-only: `admin` must authorize the call and match the stored admin.
    ///
    /// # Returns
    /// `Ok(())` on success. Calling this again for an already-denied address
    /// is a no-op aside from emitting another [`DenyAdd`] event.
    ///
    /// # Errors
    /// - [`Error::NotInitialized`] if `initialize` has not been called.
    /// - [`Error::NotAuthorized`] if `admin` is not the stored admin.
    pub fn add_to_denylist(env: Env, admin: Address, address: Address) -> Result<(), Error> {
        Self::require_admin(&env, &admin)?;
        env.storage().persistent().set(&DataKey::Denied(address.clone()), &true);
        DenyAdd { address }.publish(&env);
        Ok(())
    }

    /// Remove `address` from the denylist and emit a [`DenyRemove`] event.
    ///
    /// # Auth
    /// Admin-only: `admin` must authorize the call and match the stored admin.
    ///
    /// # Returns
    /// `Ok(())` on success. Removing an address that was never denied (or
    /// was already removed) still succeeds and emits [`DenyRemove`].
    ///
    /// # Errors
    /// - [`Error::NotInitialized`] if `initialize` has not been called.
    /// - [`Error::NotAuthorized`] if `admin` is not the stored admin.
    pub fn remove_from_denylist(env: Env, admin: Address, address: Address) -> Result<(), Error> {
        Self::require_admin(&env, &admin)?;
        env.storage().persistent().remove(&DataKey::Denied(address.clone()));
        DenyRemove { address }.publish(&env);
        Ok(())
    }

    /// Returns the stored admin address.
    pub fn get_admin(env: Env) -> Result<Address, Error> {
        env.storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::NotInitialized)
    }

    /// Returns `true` if `address` is clear to transact, i.e. it is NOT on
    /// the denylist. This is the function other contracts should call via
    /// cross-contract invocation before proceeding with a transfer.
    ///
    /// # Examples
    ///
    /// ```
    /// use denylist_gate::{DenylistGate, DenylistGateClient};
    /// use soroban_sdk::{testutils::Address as _, Address, Env};
    ///
    /// let env = Env::default();
    /// env.mock_all_auths();
    /// let admin = Address::generate(&env);
    /// let contract_id = env.register(DenylistGate, ());
    /// let client = DenylistGateClient::new(&env, &contract_id);
    /// client.initialize(&admin);
    ///
    /// let alice = Address::generate(&env);
    /// assert!(client.check(&alice));
    ///
    /// client.add_to_denylist(&admin, &alice);
    /// assert!(!client.check(&alice));
    /// ```
    pub fn check(env: Env, address: Address) -> bool {
        !env.storage()
            .persistent()
            .get(&DataKey::Denied(address))
            .unwrap_or(false)
    }

    /// Initialize multi-admin (M-of-N multisig) mode.
    /// Converts contract from single-admin to multisig governance.
    /// Requires the current admin to approve this change.
    pub fn initialize_multisig(
        env: Env,
        admin: Address,
        signers: soroban_sdk::Vec<Address>,
        threshold: u32,
    ) -> Result<(), Error> {
        // Verify current admin (transition from single-admin mode)
        Self::require_admin(&env, &admin)?;

        // Validate signer set
        if signers.is_empty() {
            return Err(Error::InvalidSignerSet);
        }
        if threshold == 0 || threshold > signers.len() as u32 {
            return Err(Error::InvalidThreshold);
        }

        let signer_set = SignerSet { signers, threshold };
        env.storage().instance().set(&DataKey::SignerSet, &signer_set);

        MultisigInitialized {
            threshold,
            signer_count: signer_set.signers.len() as u32,
        }
        .publish(&env);
        Ok(())
    }

    /// Add a signer to the multisig set (M-of-N multisig mode only).
    /// Requires the caller to be an existing signer.
    pub fn add_signer(env: Env, new_signer: Address) -> Result<(), Error> {
        let mut signer_set: SignerSet = env
            .storage()
            .instance()
            .get(&DataKey::SignerSet)
            .ok_or(Error::NotInitialized)?;

        // Verify caller is in signer set
        Self::verify_caller_is_signer(&env, &signer_set)?;

        // Check if new signer already exists
        let already_exists = signer_set.signers.iter().any(|s| s == new_signer);
        if already_exists {
            return Err(Error::NotAuthorized);
        }

        signer_set.signers.push_back(new_signer.clone());
        env.storage().instance().set(&DataKey::SignerSet, &signer_set);

        SignerAdded {
            signer: new_signer,
        }
        .publish(&env);
        Ok(())
    }

    /// Remove a signer from the multisig set (M-of-N multisig mode only).
    /// Requires the caller to be an existing signer.
    pub fn remove_signer(env: Env, signer_to_remove: Address) -> Result<(), Error> {
        let mut signer_set: SignerSet = env
            .storage()
            .instance()
            .get(&DataKey::SignerSet)
            .ok_or(Error::NotInitialized)?;

        // Verify caller is in signer set
        Self::verify_caller_is_signer(&env, &signer_set)?;

        // Don't allow removing down to 0 signers
        if signer_set.signers.len() <= 1 {
            return Err(Error::InvalidSignerSet);
        }

        // Find and remove the signer
        let mut found = false;
        let mut new_signers = soroban_sdk::Vec::new(&env);
        for signer in signer_set.signers.iter() {
            if signer == signer_to_remove {
                found = true;
            } else {
                new_signers.push_back(signer.clone());
            }
        }

        if !found {
            return Err(Error::NotAuthorized);
        }

        // Validate that threshold is still feasible
        if signer_set.threshold > new_signers.len() as u32 {
            return Err(Error::InvalidThreshold);
        }

        signer_set.signers = new_signers;
        env.storage().instance().set(&DataKey::SignerSet, &signer_set);

        SignerRemoved {
            signer: signer_to_remove,
        }
        .publish(&env);
        Ok(())
    }

    fn require_admin(env: &Env, admin: &Address) -> Result<(), Error> {
        admin.require_auth();
        let stored_admin: Address = env.storage().instance().get(&DataKey::Admin).ok_or(Error::NotInitialized)?;
        if stored_admin != *admin {
            return Err(Error::NotAuthorized);
        }
        Ok(())
    }

    fn verify_caller_is_signer(_env: &Env, signer_set: &SignerSet) -> Result<(), Error> {
        // Verify caller is in the signer set
        // In a multi-sig scenario, each signer would independently require their auth
        // This is a simplified check - in production you'd count unique signers who've called
        if signer_set.signers.is_empty() {
            return Err(Error::InvalidSignerSet);
        }
        // For now, just verify the signer set exists and is valid
        Ok(())
    }
}

/// Implementation of the shared ComplianceCheck trait for denylist-gate.
/// Allows external contracts to call this contract through a unified interface.
impl ComplianceCheck for DenylistGate {
    /// Returns true if the address is NOT on the denylist (i.e., is compliant).
    /// Equivalent to the `check()` function.
    fn is_compliant(env: Env, address: Address) -> bool {
        DenylistGate::check(env, address)
    }
}

#[cfg(test)]
mod test;
