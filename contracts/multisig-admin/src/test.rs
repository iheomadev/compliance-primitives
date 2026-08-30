use super::*;
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{vec, Env};

// ─── Helpers ─────────────────────────────────────────────────────────────────

/// Set up a 2-of-3 multisig and return (env, client, signer_a, signer_b, signer_c).
fn setup_2_of_3(
    env: &Env,
) -> (
    MultisigAdminClient<'_>,
    Address,
    Address,
    Address,
) {
    env.mock_all_auths();
    let a = Address::generate(env);
    let b = Address::generate(env);
    let c = Address::generate(env);
    let id = env.register(MultisigAdmin, ());
    let client = MultisigAdminClient::new(env, &id);
    client.initialize(&vec![env, a.clone(), b.clone(), c.clone()], &2);
    (client, a, b, c)
}

/// Set up a 1-of-1 multisig (single owner), return (env, client, owner).
fn setup_1_of_1(env: &Env) -> (MultisigAdminClient<'_>, Address) {
    env.mock_all_auths();
    let owner = Address::generate(env);
    let id = env.register(MultisigAdmin, ());
    let client = MultisigAdminClient::new(env, &id);
    client.initialize(&vec![env, owner.clone()], &1);
    (client, owner)
}

// ─── initialize ──────────────────────────────────────────────────────────────

#[test]
fn test_initialize_stores_signers_and_threshold() {
    let env = Env::default();
    let (client, a, b, c) = setup_2_of_3(&env);

    assert_eq!(client.threshold(), 2);
    let signers = client.signers();
    assert_eq!(signers.len(), 3);
    assert!(signers.iter().any(|s| s == a));
    assert!(signers.iter().any(|s| s == b));
    assert!(signers.iter().any(|s| s == c));
}

#[test]
fn test_double_initialize_fails() {
    let env = Env::default();
    let (client, a, b, _c) = setup_2_of_3(&env);
    let result = client.try_initialize(&vec![&env, a, b], &1);
    assert_eq!(result, Err(Ok(Error::AlreadyInitialized)));
}

#[test]
fn test_initialize_rejects_zero_threshold() {
    let env = Env::default();
    env.mock_all_auths();
    let a = Address::generate(&env);
    let id = env.register(MultisigAdmin, ());
    let client = MultisigAdminClient::new(&env, &id);
    let result = client.try_initialize(&vec![&env, a], &0);
    assert_eq!(result, Err(Ok(Error::InvalidThreshold)));
}

#[test]
fn test_initialize_rejects_threshold_above_signer_count() {
    let env = Env::default();
    env.mock_all_auths();
    let a = Address::generate(&env);
    let b = Address::generate(&env);
    let id = env.register(MultisigAdmin, ());
    let client = MultisigAdminClient::new(&env, &id);
    let result = client.try_initialize(&vec![&env, a, b], &3);
    assert_eq!(result, Err(Ok(Error::InvalidThreshold)));
}

// ─── propose / approve ───────────────────────────────────────────────────────

#[test]
fn test_propose_returns_id_zero() {
    let env = Env::default();
    let (client, a, _b, _c) = setup_2_of_3(&env);
    let dummy = Address::generate(&env);
    let id = client.propose(&a, &dummy, &vec![&env]);
    assert_eq!(id, 0);
}

#[test]
fn test_proposal_not_executed_until_threshold() {
    let env = Env::default();
    let (client, a, _b, _c) = setup_2_of_3(&env);
    let dummy = Address::generate(&env);
    let id = client.propose(&a, &dummy, &vec![&env]);
    // proposer auto-approves → count is 1, threshold is 2 → not yet executed
    assert_eq!(client.approval_count(&id), 1);
    let proposal = client.get_proposal(&id);
    assert!(!proposal.executed);
}

#[test]
fn test_approve_executes_at_threshold() {
    let env = Env::default();
    let (client, a, b, _c) = setup_2_of_3(&env);
    let dummy = Address::generate(&env);
    let id = client.propose(&a, &dummy, &vec![&env]);
    // second approval reaches threshold
    let executed = client.approve(&b, &id);
    assert!(executed);
    let proposal = client.get_proposal(&id);
    assert!(proposal.executed);
}

#[test]
fn test_non_signer_cannot_propose() {
    let env = Env::default();
    let (client, _a, _b, _c) = setup_2_of_3(&env);
    let outsider = Address::generate(&env);
    let dummy = Address::generate(&env);
    let result = client.try_propose(&outsider, &dummy, &vec![&env]);
    assert_eq!(result, Err(Ok(Error::NotASigner)));
}

#[test]
fn test_non_signer_cannot_approve() {
    let env = Env::default();
    let (client, a, _b, _c) = setup_2_of_3(&env);
    let dummy = Address::generate(&env);
    let id = client.propose(&a, &dummy, &vec![&env]);
    let outsider = Address::generate(&env);
    let result = client.try_approve(&outsider, &id);
    assert_eq!(result, Err(Ok(Error::NotASigner)));
}

#[test]
fn test_double_approve_fails() {
    let env = Env::default();
    let (client, a, _b, _c) = setup_2_of_3(&env);
    let dummy = Address::generate(&env);
    let id = client.propose(&a, &dummy, &vec![&env]);
    // a already approved via propose
    let result = client.try_approve(&a, &id);
    assert_eq!(result, Err(Ok(Error::AlreadyApproved)));
}

#[test]
fn test_approve_after_executed_fails() {
    let env = Env::default();
    let (client, a, b, c) = setup_2_of_3(&env);
    let dummy = Address::generate(&env);
    let id = client.propose(&a, &dummy, &vec![&env]);
    client.approve(&b, &id); // executes
    let result = client.try_approve(&c, &id);
    assert_eq!(result, Err(Ok(Error::AlreadyExecuted)));
}

// ─── propose_add_signer ──────────────────────────────────────────────────────

#[test]
fn test_add_signer_requires_threshold_approval() {
    let env = Env::default();
    let (client, a, b, _c) = setup_2_of_3(&env);
    let new_guy = Address::generate(&env);

    // Propose add — a auto-approves (count = 1, threshold = 2, not yet executed)
    let id = client.propose_add_signer(&a, &new_guy);
    assert_eq!(client.approval_count(&id), 1);
    // new_guy is not a signer yet
    assert_eq!(client.signers().len(), 3);

    // b approves — threshold met, proposal executes
    let executed = client.approve(&b, &id);
    assert!(executed);

    // new_guy is now a signer
    let signers = client.signers();
    assert_eq!(signers.len(), 4);
    assert!(signers.iter().any(|s| s == new_guy));
}

#[test]
fn test_new_signer_can_participate_after_add() {
    let env = Env::default();
    let (client, a, b, _c) = setup_2_of_3(&env);
    let new_signer = Address::generate(&env);

    // Add new_signer via threshold approval
    let add_id = client.propose_add_signer(&a, &new_signer);
    client.approve(&b, &add_id); // executes, new_signer added

    // Now open a new proposal
    let dummy = Address::generate(&env);
    let prop_id = client.propose(&a, &dummy, &vec![&env]); // a auto-approves (count=1)

    // new_signer can approve — threshold=2, count reaches 2 → executes
    let executed = client.approve(&new_signer, &prop_id);
    assert!(executed);
    assert!(client.get_proposal(&prop_id).executed);
}

#[test]
fn test_add_signer_immediate_in_1_of_1() {
    let env = Env::default();
    let (client, owner) = setup_1_of_1(&env);
    let new_signer = Address::generate(&env);

    let id = client.propose_add_signer(&owner, &new_signer);
    // threshold=1, auto-approved → should execute immediately
    let proposal = client.get_proposal(&id);
    assert!(proposal.executed);
    assert_eq!(client.signers().len(), 2);
}

// ─── propose_remove_signer ───────────────────────────────────────────────────

#[test]
fn test_remove_signer_requires_threshold_approval() {
    let env = Env::default();
    let (client, a, b, c) = setup_2_of_3(&env);

    // Propose removing c — a auto-approves (count=1, not yet executed)
    let id = client.propose_remove_signer(&a, &c);
    assert_eq!(client.signers().len(), 3); // c still in set

    // b approves — threshold met
    let executed = client.approve(&b, &id);
    assert!(executed);

    let signers = client.signers();
    assert_eq!(signers.len(), 2);
    assert!(!signers.iter().any(|s| s == c));
}

#[test]
fn test_removed_signer_prior_approvals_no_longer_count() {
    let env = Env::default();
    // 3-of-3 multisig so we need all three approvals
    env.mock_all_auths();
    let a = Address::generate(&env);
    let b = Address::generate(&env);
    let c = Address::generate(&env);
    let id = env.register(MultisigAdmin, ());
    let client = MultisigAdminClient::new(&env, &id);
    client.initialize(&vec![&env, a.clone(), b.clone(), c.clone()], &3);

    // Open a generic proposal — a and b approve (count=2, need 3)
    let dummy = Address::generate(&env);
    let prop_id = client.propose(&a, &dummy, &vec![&env]); // a: 1
    client.approve(&b, &prop_id); // b: 2 — still need c

    // Now remove c via a separate 3-of-3 removal proposal.
    // All three must approve to remove c; then c is gone.
    let remove_id = client.propose_remove_signer(&a, &c); // a: 1
    client.approve(&b, &remove_id); // b: 2
    client.approve(&c, &remove_id); // c: 3 → threshold met, c removed

    // c is no longer a signer
    assert!(!client.signers().iter().any(|s| s == c));

    // c's prior approval on prop_id should have been revoked.
    // The original proposal still needs the third approval, but c is gone.
    // Count on prop_id should now reflect only a and b (c's approval was revoked).
    assert_eq!(client.approval_count(&prop_id), 2);

    // Proposal is still pending (not executed) because threshold was 3 and
    // c's approval was revoked. The remaining signers (a, b) are below 3.
    let proposal = client.get_proposal(&prop_id);
    assert!(!proposal.executed);
}

#[test]
fn test_remove_signer_fails_when_would_drop_below_threshold() {
    let env = Env::default();
    env.mock_all_auths();
    let a = Address::generate(&env);
    let b = Address::generate(&env);
    let id = env.register(MultisigAdmin, ());
    let client = MultisigAdminClient::new(&env, &id);
    // 2-of-2: removing either signer would leave only 1, which is < threshold 2
    client.initialize(&vec![&env, a.clone(), b.clone()], &2);

    let propose_id = client.propose_remove_signer(&a, &b);
    // a auto-approves (1 of 2), b approves → threshold met, but execution should fail
    let result = client.try_approve(&b, &propose_id);
    assert_eq!(result, Err(Ok(Error::SignerSetTooSmall)));
}
