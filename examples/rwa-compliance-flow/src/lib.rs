//! Integration example: a full RWA token flow using all three compliance contracts together.
//!
//! This example demonstrates:
//! 1. A successful token transfer when all three compliance checks pass
//! 2. Blocked transfers when each compliance check independently fails
//!
//! **Key Learning**: The three contracts (`allowlist-token`, `denylist-gate`,
//! `jurisdiction-flag`) can be composed to create a comprehensive compliance
//! framework for RWA tokens and stablecoins.
//!
//! **Compliance Layers**:
//! - **Allowlist**: Only allowlisted addresses can transact
//! - **Denylist**: Sanctioned, defrauded, or court-ordered addresses are blocked
//! - **Jurisdiction**: Restricts activity to permitted jurisdictions (e.g., regulatory
//!   requirements)

#![no_std]

use soroban_sdk::{Address, Env, String};

// Contract clients generated from trait definitions
// These allow us to call the three compliance contracts without linking their full implementations
use soroban_sdk::contractclient;

/// Shared compliance check interface - unified way to call any compliance contract.
/// All three primitives implement this trait with `is_compliant(address) -> bool`.
#[contractclient(name = "ComplianceCheckClient")]
pub trait ComplianceCheckInterface {
    fn is_compliant(env: Env, address: Address) -> bool;
}

#[contractclient(name = "AllowlistTokenClient")]
pub trait AllowlistTokenInterface {
    fn initialize(env: Env, admin: Address, token: Address) -> Result<(), soroban_sdk::contracterror::ContractError>;
    fn add_to_allowlist(env: Env, admin: Address, address: Address) -> Result<(), soroban_sdk::contracterror::ContractError>;
    fn remove_from_allowlist(env: Env, admin: Address, address: Address) -> Result<(), soroban_sdk::contracterror::ContractError>;
    fn is_allowed(env: Env, address: Address) -> bool;
    fn is_compliant(env: Env, address: Address) -> bool;
}

#[contractclient(name = "DenylistGateClient")]
pub trait DenylistGateInterface {
    fn initialize(env: Env, admin: Address) -> Result<(), soroban_sdk::contracterror::ContractError>;
    fn add_to_denylist(env: Env, admin: Address, address: Address) -> Result<(), soroban_sdk::contracterror::ContractError>;
    fn remove_from_denylist(env: Env, admin: Address, address: Address) -> Result<(), soroban_sdk::contracterror::ContractError>;
    fn check(env: Env, address: Address) -> bool;
    fn is_compliant(env: Env, address: Address) -> bool;
}

#[contractclient(name = "JurisdictionFlagClient")]
pub trait JurisdictionFlagInterface {
    fn initialize(env: Env, issuer: Address) -> Result<(), soroban_sdk::contracterror::ContractError>;
    fn set_jurisdiction(env: Env, issuer: Address, address: Address, code: String) -> Result<(), soroban_sdk::contracterror::ContractError>;
    fn get_jurisdiction(env: Env, address: Address) -> Option<String>;
    fn is_permitted_jurisdiction(env: Env, address: Address, allowed_codes: soroban_sdk::Vec<String>) -> bool;
    fn is_compliant(env: Env, address: Address) -> bool;
}

#[cfg(test)]
mod test;
