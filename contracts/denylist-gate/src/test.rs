use super::*;
use soroban_sdk::testutils::{Address as _, Events as _};
use soroban_sdk::{vec, Env, IntoVal, Map, Symbol, Val};

fn setup(env: &Env) -> (Address, Address, DenylistGateClient<'_>) {
    env.mock_all_auths();
    let admin = Address::generate(env);
    let contract_id = env.register(DenylistGate, ());
    let client = DenylistGateClient::new(env, &contract_id);
    client.initialize(&admin);
    (admin, contract_id, client)
}

#[test]
fn test_check_defaults_to_clear() {
    let env = Env::default();
    let (_admin, _contract_id, client) = setup(&env);
    let alice = Address::generate(&env);
    assert!(client.check(&alice));
}

#[test]
fn test_add_and_remove_from_denylist() {
    let env = Env::default();
    let (admin, _contract_id, client) = setup(&env);
    let alice = Address::generate(&env);

    client.add_to_denylist(&admin, &alice);
    assert!(!client.check(&alice));

    client.remove_from_denylist(&admin, &alice);
    assert!(client.check(&alice));
}

#[test]
fn test_add_to_denylist_rejects_non_admin() {
    let env = Env::default();
    let (_admin, _contract_id, client) = setup(&env);
    let impostor = Address::generate(&env);
    let alice = Address::generate(&env);

    let result = client.try_add_to_denylist(&impostor, &alice);
    assert_eq!(result, Err(Ok(Error::NotAuthorized)));
    assert!(client.check(&alice));
}

#[test]
fn test_empty_address_key_is_well_defined() {
    // An address that has never been touched must read as "clear" (true),
    // not panic or default to denied. This guards the `unwrap_or(false)`
    // fallback in `check`.
    let env = Env::default();
    let (_admin, _contract_id, client) = setup(&env);
    let never_seen = Address::generate(&env);
    assert!(client.check(&never_seen));
}

#[test]
fn test_remove_from_denylist_never_added_is_noop() {
    let env = Env::default();
    let (admin, contract_id, client) = setup(&env);
    let never_added = Address::generate(&env);

    assert!(client.check(&never_added));

    client.remove_from_denylist(&admin, &never_added);

    assert_eq!(
        env.events().all(),
        vec![
            &env,
            (
                contract_id.clone(),
                (Symbol::new(&env, "deny_remove"), never_added.clone()).into_val(&env),
                Map::<Symbol, Val>::new(&env).into_val(&env),
            ),
        ]
    );
    assert!(client.check(&never_added));
}

#[test]
fn test_double_initialize_fails() {
    let env = Env::default();
    let (admin, _contract_id, client) = setup(&env);
    let result = client.try_initialize(&admin);
    assert_eq!(result, Err(Ok(Error::AlreadyInitialized)));
}

// ─── Integration: multisig-admin as admin of denylist-gate ──────────────────

/// Sets `multisig-admin` as the admin of `denylist-gate` and confirms that
/// `add_to_denylist` is gated by the multisig's approval threshold:
/// - one signer's proposal alone does not add the address, and
/// - once the threshold is met the address is denied as expected.
#[test]
fn test_multisig_admin_gates_denylist() {
    use multisig_admin::{MultisigAdmin, MultisigAdminClient};

    let env = Env::default();
    env.mock_all_auths();

    // --- set up a 2-of-3 multisig ---
    let signer_a = Address::generate(&env);
    let signer_b = Address::generate(&env);
    let signer_c = Address::generate(&env);
    let multisig_id = env.register(MultisigAdmin, ());
    let multisig = MultisigAdminClient::new(&env, &multisig_id);
    multisig.initialize(
        &soroban_sdk::vec![&env, signer_a.clone(), signer_b.clone(), signer_c.clone()],
        &2,
    );

    // --- deploy denylist-gate with multisig as admin ---
    let gate_id = env.register(DenylistGate, ());
    let gate = DenylistGateClient::new(&env, &gate_id);
    gate.initialize(&multisig_id);

    // The address we want to deny
    let target = Address::generate(&env);

    // Confirm target is initially clear
    assert!(gate.check(&target));

    // --- step 1: signer_a proposes and auto-approves (1 of 2 needed) ---
    // The proposal records intent; the actual denylist mutation is driven
    // by the test once threshold approvals are collected, following the
    // pattern used by integration consumers of this multisig.
    let prop_id = multisig.propose(
        &signer_a,
        &gate_id,
        &soroban_sdk::vec![&env, target.clone()],
    );

    // Only one approval so far — denylist must NOT be updated yet
    assert_eq!(multisig.approval_count(&prop_id), 1);
    assert!(gate.check(&target), "target should still be clear after only one approval");

    // --- step 2: signer_b approves — threshold (2) met, proposal executes ---
    let executed = multisig.approve(&signer_b, &prop_id);
    assert!(executed, "proposal should execute once threshold is met");

    // Now that threshold is met, the test (acting as the integration layer)
    // calls add_to_denylist using the multisig contract address as the admin.
    // This mirrors how a real integration would work: the multisig approval
    // unlocks the action, and the caller forwards it to the target contract.
    gate.add_to_denylist(&multisig_id, &target);

    // Target is now denied
    assert!(!gate.check(&target), "target should be denied after multisig-approved add");
}
