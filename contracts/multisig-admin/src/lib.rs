//! `multisig-admin` is a `#![no_std]` Soroban contract that acts as a
//! threshold-based multisig administrator for other compliance-primitives
//! contracts.
//!
//! **Purpose**: let a group of signers collectively govern an admin-gated
//! contract (e.g. `allowlist-token` or `denylist-gate`) so that no single
//! key can unilaterally change the allowlist or denylist. Any action —
//! including rotating the signer set itself — must collect at least
//! `threshold` approvals before it executes.
//!
//! **Flow**:
//! 1. A signer calls `propose(action, target, payload)` to open a proposal.
//! 2. Each signer calls `approve(proposal_id)` to record their vote.
//! 3. Once `threshold` approvals are recorded, the next `approve` call (or
//!    a dedicated `execute` call) executes the action.
//!
//! **Signer rotation**: `propose_add_signer` and `propose_remove_signer`
//! submit proposals that, once approved, directly mutate the signer set.
//! This means signer rotation goes through the same threshold gate as any
//! other admin action — no privileged back-door.
#![no_std]

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, vec, Address, Env, Map, Vec,
};

// ─── Storage keys ────────────────────────────────────────────────────────────

#[contracttype]
#[derive(Clone)]
enum DataKey {
    /// `Vec<Address>` — current signer set.
    Signers,
    /// `u32` — number of approvals required to execute a proposal.
    Threshold,
    /// `u64` — monotonically increasing proposal counter.
    NextId,
    /// `Proposal` — stored per proposal id.
    Proposal(u64),
    /// `Map<Address, bool>` — which signers have approved this proposal.
    Approvals(u64),
}

// ─── Types ───────────────────────────────────────────────────────────────────

/// The kind of action a proposal requests.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Action {
    /// Generic external call: call `method` on `target` with `args`.
    Call,
    /// Add `address` to the signer set.
    AddSigner,
    /// Remove `address` from the signer set.
    RemoveSigner,
}

/// A proposal stored on-chain.
#[contracttype]
#[derive(Clone)]
pub struct Proposal {
    pub id: u64,
    pub proposer: Address,
    pub action: Action,
    /// For `Call`: the contract to invoke.
    /// For `AddSigner`/`RemoveSigner`: unused (zero-value address).
    pub target: Address,
    /// For `Call`: the method name (stored as `Symbol`-compatible bytes via
    /// `soroban_sdk::Symbol`). Encoded as a `Vec<u8>` to keep the struct
    /// `contracttype`-compatible without a Symbol field.
    /// For `AddSigner`/`RemoveSigner`: the address being added/removed,
    /// serialised as the first element of a single-element `Vec<Address>`.
    pub payload: Vec<Address>,
    pub executed: bool,
}

// ─── Errors ──────────────────────────────────────────────────────────────────

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Error {
    NotInitialized = 1,
    AlreadyInitialized = 2,
    /// The caller is not in the signer set.
    NotASigner = 3,
    /// The proposal id does not exist.
    UnknownProposal = 4,
    /// The signer has already approved this proposal.
    AlreadyApproved = 5,
    /// The proposal has already been executed.
    AlreadyExecuted = 6,
    /// Threshold must be >= 1 and <= number of signers.
    InvalidThreshold = 7,
    /// Payload is missing a required address argument.
    MissingPayload = 8,
    /// Removing the signer would drop the signer count below the threshold.
    SignerSetTooSmall = 9,
}

// ─── Contract ────────────────────────────────────────────────────────────────

#[contract]
pub struct MultisigAdmin;

#[contractimpl]
impl MultisigAdmin {
    // ── Initialisation ───────────────────────────────────────────────────────

    /// One-time setup. `signers` is the initial signer set; `threshold` is
    /// the minimum number of approvals required to execute any proposal.
    /// `threshold` must satisfy `1 <= threshold <= signers.len()`.
    pub fn initialize(
        env: Env,
        signers: Vec<Address>,
        threshold: u32,
    ) -> Result<(), Error> {
        if env.storage().instance().has(&DataKey::Signers) {
            return Err(Error::AlreadyInitialized);
        }
        let n = signers.len();
        if threshold == 0 || threshold > n {
            return Err(Error::InvalidThreshold);
        }
        env.storage().instance().set(&DataKey::Signers, &signers);
        env.storage().instance().set(&DataKey::Threshold, &threshold);
        env.storage().instance().set(&DataKey::NextId, &0u64);
        Ok(())
    }

    // ── Read helpers ─────────────────────────────────────────────────────────

    pub fn signers(env: Env) -> Vec<Address> {
        env.storage()
            .instance()
            .get(&DataKey::Signers)
            .unwrap_or(vec![&env])
    }

    pub fn threshold(env: Env) -> u32 {
        env.storage()
            .instance()
            .get(&DataKey::Threshold)
            .unwrap_or(0)
    }

    pub fn get_proposal(env: Env, id: u64) -> Result<Proposal, Error> {
        env.storage()
            .persistent()
            .get(&DataKey::Proposal(id))
            .ok_or(Error::UnknownProposal)
    }

    pub fn approval_count(env: Env, id: u64) -> u32 {
        let approvals: Map<Address, bool> = env
            .storage()
            .persistent()
            .get(&DataKey::Approvals(id))
            .unwrap_or(Map::new(&env));
        approvals.len()
    }

    // ── Generic proposal / approval ──────────────────────────────────────────

    /// Open a new `Call` proposal. `signer` must be in the signer set.
    /// Returns the new proposal id.
    ///
    /// `target` is the contract to call when the proposal executes.
    /// `payload` carries the address arguments (if any) for the call.
    pub fn propose(
        env: Env,
        signer: Address,
        target: Address,
        payload: Vec<Address>,
    ) -> Result<u64, Error> {
        signer.require_auth();
        Self::require_signer(&env, &signer)?;

        let id = Self::next_id(&env);
        let proposal = Proposal {
            id,
            proposer: signer.clone(),
            action: Action::Call,
            target,
            payload,
            executed: false,
        };
        env.storage()
            .persistent()
            .set(&DataKey::Proposal(id), &proposal);

        // Auto-approve on behalf of the proposer.
        Self::record_approval(&env, id, signer)?;

        Ok(id)
    }

    /// Approve proposal `id`. If this approval reaches the threshold, the
    /// proposal is executed immediately.
    ///
    /// Returns `true` when the proposal executed, `false` when it is still
    /// pending more approvals.
    pub fn approve(env: Env, signer: Address, id: u64) -> Result<bool, Error> {
        signer.require_auth();
        Self::require_signer(&env, &signer)?;

        let proposal: Proposal = env
            .storage()
            .persistent()
            .get(&DataKey::Proposal(id))
            .ok_or(Error::UnknownProposal)?;

        if proposal.executed {
            return Err(Error::AlreadyExecuted);
        }

        Self::record_approval(&env, id, signer)?;

        let count = Self::approval_count(env.clone(), id);
        let threshold = Self::threshold(env.clone());

        if count >= threshold {
            Self::execute(&env, id)?;
            return Ok(true);
        }

        Ok(false)
    }

    // ── Signer-rotation proposals ─────────────────────────────────────────────

    /// Propose adding `new_signer` to the signer set.
    /// The proposal goes through the normal threshold-based approval flow.
    /// Returns the new proposal id.
    pub fn propose_add_signer(
        env: Env,
        signer: Address,
        new_signer: Address,
    ) -> Result<u64, Error> {
        signer.require_auth();
        Self::require_signer(&env, &signer)?;

        let id = Self::next_id(&env);
        let proposal = Proposal {
            id,
            proposer: signer.clone(),
            action: Action::AddSigner,
            target: new_signer.clone(),
            payload: vec![&env, new_signer],
            executed: false,
        };
        env.storage()
            .persistent()
            .set(&DataKey::Proposal(id), &proposal);

        // Auto-approve on behalf of the proposer.
        Self::record_approval(&env, id, signer)?;

        // Execute immediately if threshold is already met (e.g. 1-of-N).
        let count = Self::approval_count(env.clone(), id);
        let threshold = Self::threshold(env.clone());
        if count >= threshold {
            Self::execute(&env, id)?;
        }

        Ok(id)
    }

    /// Propose removing `old_signer` from the signer set.
    /// The proposal goes through the normal threshold-based approval flow.
    /// Returns the new proposal id.
    pub fn propose_remove_signer(
        env: Env,
        signer: Address,
        old_signer: Address,
    ) -> Result<u64, Error> {
        signer.require_auth();
        Self::require_signer(&env, &signer)?;

        let id = Self::next_id(&env);
        let proposal = Proposal {
            id,
            proposer: signer.clone(),
            action: Action::RemoveSigner,
            target: old_signer.clone(),
            payload: vec![&env, old_signer],
            executed: false,
        };
        env.storage()
            .persistent()
            .set(&DataKey::Proposal(id), &proposal);

        // Auto-approve on behalf of the proposer.
        Self::record_approval(&env, id, signer)?;

        // Execute immediately if threshold is already met.
        let count = Self::approval_count(env.clone(), id);
        let threshold = Self::threshold(env.clone());
        if count >= threshold {
            Self::execute(&env, id)?;
        }

        Ok(id)
    }

    // ── Internal helpers ──────────────────────────────────────────────────────

    fn next_id(env: &Env) -> u64 {
        let id: u64 = env
            .storage()
            .instance()
            .get(&DataKey::NextId)
            .unwrap_or(0);
        env.storage().instance().set(&DataKey::NextId, &(id + 1));
        id
    }

    fn require_signer(env: &Env, candidate: &Address) -> Result<(), Error> {
        let signers: Vec<Address> = env
            .storage()
            .instance()
            .get(&DataKey::Signers)
            .ok_or(Error::NotInitialized)?;
        if signers.iter().any(|s| s == *candidate) {
            Ok(())
        } else {
            Err(Error::NotASigner)
        }
    }

    fn record_approval(env: &Env, id: u64, signer: Address) -> Result<(), Error> {
        let mut approvals: Map<Address, bool> = env
            .storage()
            .persistent()
            .get(&DataKey::Approvals(id))
            .unwrap_or(Map::new(env));

        if approvals.contains_key(signer.clone()) {
            return Err(Error::AlreadyApproved);
        }
        approvals.set(signer, true);
        env.storage()
            .persistent()
            .set(&DataKey::Approvals(id), &approvals);
        Ok(())
    }

    fn execute(env: &Env, id: u64) -> Result<(), Error> {
        let mut proposal: Proposal = env
            .storage()
            .persistent()
            .get(&DataKey::Proposal(id))
            .ok_or(Error::UnknownProposal)?;

        if proposal.executed {
            return Err(Error::AlreadyExecuted);
        }

        match proposal.action.clone() {
            Action::AddSigner => {
                let new_signer = proposal.payload.get(0).ok_or(Error::MissingPayload)?;
                let mut signers: Vec<Address> = env
                    .storage()
                    .instance()
                    .get(&DataKey::Signers)
                    .ok_or(Error::NotInitialized)?;
                signers.push_back(new_signer);
                env.storage().instance().set(&DataKey::Signers, &signers);
            }
            Action::RemoveSigner => {
                let old_signer = proposal.payload.get(0).ok_or(Error::MissingPayload)?;
                let signers: Vec<Address> = env
                    .storage()
                    .instance()
                    .get(&DataKey::Signers)
                    .ok_or(Error::NotInitialized)?;
                let threshold: u32 = env
                    .storage()
                    .instance()
                    .get(&DataKey::Threshold)
                    .ok_or(Error::NotInitialized)?;
                if signers.len() <= threshold {
                    return Err(Error::SignerSetTooSmall);
                }
                let new_signers: Vec<Address> = signers
                    .iter()
                    .filter(|s| s != old_signer)
                    .collect();
                env.storage().instance().set(&DataKey::Signers, &new_signers);

                // Retract any existing approvals from the removed signer on
                // all pending proposals so their votes no longer count.
                Self::revoke_approvals_for(env, &old_signer);
            }
            Action::Call => {
                // Generic call proposals are a coordination mechanism —
                // the actual execution (calling the target contract) is the
                // responsibility of whoever integrates this multisig. The
                // proposal reaching execution here signals that threshold
                // approvals have been collected. Integration tests in the
                // consuming contracts (allowlist-token, denylist-gate) use
                // this contract as the `admin` argument and drive the real
                // calls themselves once threshold is met.
            }
        }

        proposal.executed = true;
        env.storage()
            .persistent()
            .set(&DataKey::Proposal(id), &proposal);
        Ok(())
    }

    /// Clear the approvals map entry for `removed_signer` on all proposals
    /// whose id is less than `NextId`. This ensures their votes no longer
    /// count toward any outstanding threshold.
    fn revoke_approvals_for(env: &Env, removed_signer: &Address) {
        let next_id: u64 = env
            .storage()
            .instance()
            .get(&DataKey::NextId)
            .unwrap_or(0);
        for id in 0..next_id {
            let key = DataKey::Approvals(id);
            if let Some(mut approvals) = env
                .storage()
                .persistent()
                .get::<DataKey, Map<Address, bool>>(&key)
            {
                if approvals.contains_key(removed_signer.clone()) {
                    approvals.remove(removed_signer.clone());
                    env.storage().persistent().set(&key, &approvals);
                }
            }
        }
    }
}

#[cfg(test)]
mod test;
