//! `pausable` is a `#![no_std]` shared crate that provides a minimal pause
//! mechanism for Soroban contracts.
//!
//! **Purpose**: let any Soroban contract add an emergency-stop capability
//! without reimplementing the same storage key and guard logic. A consuming
//! contract stores a single boolean under a well-known key and calls these
//! helpers to manage it.
//!
//! **Usage**: import this crate as a regular dependency and call the free
//! functions directly, passing your contract's `Env`. No contract struct is
//! defined here — these are pure utility functions, not an entry-point
//! contract.
#![no_std]

use soroban_sdk::{contracttype, Env};

#[contracttype]
#[derive(Clone)]
enum DataKey {
    Paused,
}

/// Returns `true` if the contract is currently paused.
///
/// Defaults to `false` when no pause flag has been stored yet, so a freshly
/// initialized contract starts in the active (unpaused) state.
pub fn is_paused(env: &Env) -> bool {
    env.storage()
        .instance()
        .get(&DataKey::Paused)
        .unwrap_or(false)
}

/// Set the paused flag to `true`.
///
/// Calling this when already paused is a no-op — the flag remains set.
pub fn pause(env: &Env) {
    env.storage().instance().set(&DataKey::Paused, &true);
}

/// Clear the paused flag, returning the contract to the active state.
///
/// Calling this when not paused is a no-op — the flag remains cleared.
pub fn unpause(env: &Env) {
    env.storage().instance().set(&DataKey::Paused, &false);
}

/// Panic with a descriptive message if the contract is currently paused.
///
/// Consuming contracts should call this at the top of any state-mutating
/// entry point that must be blocked while paused, e.g.:
///
/// ```ignore
/// pub fn transfer(env: Env, ...) -> Result<(), Error> {
///     pausable::require_not_paused(&env);
///     // ...
/// }
/// ```
pub fn require_not_paused(env: &Env) {
    if is_paused(env) {
        panic!("contract is paused");
    }
}

#[cfg(test)]
mod test;
