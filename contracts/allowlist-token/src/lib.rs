//! `allowlist-token` is a `#![no_std]` Soroban contract that wraps an existing
//! SEP-41 token and only permits `transfer` calls between two addresses that
//! are both present on an on-chain allowlist.
//!
//! **Purpose**: give issuers of permissioned tokens (e.g. RWA or regulated
//! stablecoins) a drop-in gate that blocks transfers to or from addresses
//! that haven't cleared KYC/onboarding, without modifying the underlying
//! token contract's own logic.
//!
//! **Callers**: an `admin` address manages the allowlist through
//! `add_to_allowlist`/`remove_from_allowlist`. End users — or the wallets
//! and apps acting on their behalf — call `transfer` exactly as they would
//! on a plain SEP-41 token; the allowlist check happens transparently.
//!
//! **Composition**: deploy this contract in front of an issuer's real token
//! and point clients at it instead of the underlying token — cleared
//! transfers are forwarded on via a cross-contract call. This is the one
//! primitive in the workspace meant to be deployed standalone rather than
//! called into by another contract; contrast with `denylist-gate` and
//! `jurisdiction-flag`, which are designed to be composed into a caller's
//! own token contract.
#![no_std]

use soroban_sdk::{contract, contracterror, contractevent, contractimpl, contracttype, token, Address, Env};

/// Extend a persistent allowlist entry when its remaining TTL drops below
/// this many ledgers (~7 days at ~5s/ledger on mainnet).
///
/// Chosen so an idle KYC'd address still gets renewed well before archival,
/// without paying for an extension on every recent write.
pub(crate) const ALLOWED_TTL_THRESHOLD: u32 = 120_960; // ~7 days

/// Target remaining TTL after extension (~90 days at ~5s/ledger).
///
/// Long enough that an issuer can go weeks without touching an entry and
/// still avoid archival, while staying under typical network `max_entry_ttl`.
pub(crate) const ALLOWED_TTL_EXTEND_TO: u32 = 1_555_200; // ~90 days

#[contracttype]
#[derive(Clone)]
enum DataKey {
    Admin,
    Token,
    Allowed(Address),
}

#[contractevent]
pub struct AllowAdd {
    #[topic]
    pub address: Address,
}

#[contractevent]
pub struct AllowRemove {
    #[topic]
    pub address: Address,
}

#[contractevent]
pub struct Blocked {
    #[topic]
    pub from: Address,
    #[topic]
    pub to: Address,
    pub amount: i128,
}

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Error {
    NotInitialized = 1,
    AlreadyInitialized = 2,
    NotAuthorized = 3,
}

#[contract]
pub struct AllowlistToken;

#[contractimpl]
impl AllowlistToken {
    /// One-time setup. `admin` may manage the allowlist; `token` is the
    /// address of the underlying SEP-41 token contract that real transfers
    /// are forwarded to once both parties clear the allowlist check.
    pub fn initialize(env: Env, admin: Address, token: Address) -> Result<(), Error> {
        if env.storage().instance().has(&DataKey::Admin) {
            return Err(Error::AlreadyInitialized);
        }
        admin.require_auth();
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::Token, &token);
        Ok(())
    }

    /// Add `address` to the allowlist. Admin-only.
    ///
    /// After writing the persistent entry, extends its TTL to
    /// [`ALLOWED_TTL_EXTEND_TO`] when the remaining TTL is below
    /// [`ALLOWED_TTL_THRESHOLD`].
    ///
    /// **TTL tradeoff (write-only vs read-triggered):** extension runs only
    /// on allowlist writes here, not on `is_allowed` / `transfer` reads.
    /// Write-only keeps transfer paths cheaper (no TTL bump fee on every
    /// gated transfer) and matches the issuer's mutation cadence. The cost
    /// is that a never-mutated entry can still approach archival if nothing
    /// re-adds it for ~90 days — issuers that need read-side keep-alive
    /// should bump TTL from an off-chain renewal job or revisit adding
    /// read-triggered `extend_ttl` later.
    pub fn add_to_allowlist(env: Env, admin: Address, address: Address) -> Result<(), Error> {
        Self::require_admin(&env, &admin)?;
        let key = DataKey::Allowed(address.clone());
        env.storage().persistent().set(&key, &true);
        env.storage().persistent().extend_ttl(
            &key,
            ALLOWED_TTL_THRESHOLD,
            ALLOWED_TTL_EXTEND_TO,
        );
        AllowAdd { address }.publish(&env);
        Ok(())
    }

    /// Remove `address` from the allowlist. Admin-only.
    pub fn remove_from_allowlist(env: Env, admin: Address, address: Address) -> Result<(), Error> {
        Self::require_admin(&env, &admin)?;
        env.storage()
            .persistent()
            .remove(&DataKey::Allowed(address.clone()));
        AllowRemove { address }.publish(&env);
        Ok(())
    }

    /// Returns true if `address` is currently allowlisted.
    pub fn is_allowed(env: Env, address: Address) -> bool {
        env.storage()
            .persistent()
            .get(&DataKey::Allowed(address))
            .unwrap_or(false)
    }

    /// Transfer `amount` of the underlying token from `from` to `to`.
    ///
    /// Returns `Ok(false)` without forwarding the transfer if either party is
    /// not allowlisted, and emits a `Blocked` event so the attempt is
    /// auditable off-chain. A Soroban invocation that returns a contract
    /// error rolls back everything it did, including events, so a blocked
    /// attempt is reported as `Ok(false)` rather than an `Err` — that's what
    /// lets the audit event actually land. `Err` is reserved for
    /// configuration failures (e.g. the contract was never initialized).
    pub fn transfer(env: Env, from: Address, to: Address, amount: i128) -> Result<bool, Error> {
        from.require_auth();

        if !Self::is_allowed(env.clone(), from.clone()) || !Self::is_allowed(env.clone(), to.clone()) {
            Blocked { from, to, amount }.publish(&env);
            return Ok(false);
        }

        let token_address: Address = env
            .storage()
            .instance()
            .get(&DataKey::Token)
            .ok_or(Error::NotInitialized)?;
        let token_client = token::Client::new(&env, &token_address);
        token_client.transfer(&from, &to, &amount);
        Ok(true)
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
