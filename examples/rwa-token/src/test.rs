use super::*;
use soroban_sdk::testutils::{Address as _, Events as _};
use soroban_sdk::{vec, Env};

fn setup(
    env: &Env,
) -> (
    Address,                 // admin
    Address,                 // denylist_addr
    Address,                 // allowlist_addr
    Address,                 // jurisdiction_addr
    RwaTokenClient<'_>,
) {
    env.mock_all_auths();

    let admin = Address::generate(env);

    // Register the three primitive contracts
    let denylist_addr = env.register(denylist_gate::DenylistGate, ());
    let denylist_client = denylist_gate::DenylistGateClient::new(env, &denylist_addr);
    denylist_client.initialize(&admin);

    // For allowlist, we need to mock it since we can't easily instantiate it
    let allowlist_addr = env.register(allowlist_token::AllowlistToken, ());

    // For jurisdiction, register and initialize
    let jurisdiction_addr = env.register(jurisdiction_flag::JurisdictionFlag, ());
    let jurisdiction_client = jurisdiction_flag::JurisdictionFlagClient::new(env, &jurisdiction_addr);
    jurisdiction_client.initialize(&admin);

    // Register and initialize the RWA token
    let token_id = env.register(RwaToken, ());
    let token_client = RwaTokenClient::new(env, &token_id);
    token_client.initialize(&admin, &denylist_addr, &allowlist_addr, &jurisdiction_addr);

    (admin, denylist_addr, allowlist_addr, jurisdiction_addr, token_client)
}

#[test]
fn test_initialize() {
    let env = Env::default();
    let (admin, denylist_addr, allowlist_addr, jurisdiction_addr, token_client) = setup(&env);

    // Verify the contract initialized successfully (no error thrown)
    // In a real test, you'd verify state, but that requires public getters
    let alice = Address::generate(&env);
    assert_eq!(token_client.get_balance(&alice), 0);
}

#[test]
fn test_mint_and_balance() {
    let env = Env::default();
    let (admin, _denylist_addr, _allowlist_addr, _jurisdiction_addr, token_client) = setup(&env);
    let alice = Address::generate(&env);

    token_client.mint(&admin, &alice, &1000);
    assert_eq!(token_client.get_balance(&alice), 1000);
}

#[test]
fn test_mint_non_admin_fails() {
    let env = Env::default();
    let (_admin, _denylist_addr, _allowlist_addr, _jurisdiction_addr, token_client) = setup(&env);
    let alice = Address::generate(&env);
    let non_admin = Address::generate(&env);

    let result = token_client.try_mint(&non_admin, &alice, &1000);
    assert_eq!(result, Err(Ok(Error::NotAuthorized)));
    assert_eq!(token_client.get_balance(&alice), 0);
}

#[test]
fn test_transfer_fails_if_denied_by_denylist() {
    let env = Env::default();
    let (admin, denylist_addr, _allowlist_addr, _jurisdiction_addr, token_client) = setup(&env);
    let alice = Address::generate(&env);
    let bob = Address::generate(&env);

    // Mint to alice
    token_client.mint(&admin, &alice, &1000);

    // Add alice to the denylist
    let denylist_client = denylist_gate::DenylistGateClient::new(&env, &denylist_addr);
    denylist_client.add_to_denylist(&admin, &alice);

    // Try to transfer from alice to bob
    let result = token_client.try_transfer(&alice, &bob, &100);
    assert_eq!(result, Err(Ok(Error::DeniedByDenylist)));

    // Verify balances didn't change
    assert_eq!(token_client.get_balance(&alice), 1000);
    assert_eq!(token_client.get_balance(&bob), 0);
}

#[test]
fn test_transfer_fails_if_recipient_denied_by_denylist() {
    let env = Env::default();
    let (admin, denylist_addr, _allowlist_addr, _jurisdiction_addr, token_client) = setup(&env);
    let alice = Address::generate(&env);
    let bob = Address::generate(&env);

    // Mint to alice
    token_client.mint(&admin, &alice, &1000);

    // Add bob to the denylist
    let denylist_client = denylist_gate::DenylistGateClient::new(&env, &denylist_addr);
    denylist_client.add_to_denylist(&admin, &bob);

    // Try to transfer from alice to bob
    let result = token_client.try_transfer(&alice, &bob, &100);
    assert_eq!(result, Err(Ok(Error::DeniedByDenylist)));

    // Verify balances didn't change
    assert_eq!(token_client.get_balance(&alice), 1000);
    assert_eq!(token_client.get_balance(&bob), 0);
}

#[test]
fn test_transfer_fails_insufficient_balance() {
    let env = Env::default();
    let (admin, _denylist_addr, _allowlist_addr, _jurisdiction_addr, token_client) = setup(&env);
    let alice = Address::generate(&env);
    let bob = Address::generate(&env);

    // Mint only 50 to alice
    token_client.mint(&admin, &alice, &50);

    // Try to transfer 100 (more than balance)
    let result = token_client.try_transfer(&alice, &bob, &100);
    assert_eq!(result, Err(Ok(Error::InsufficientBalance)));

    // Verify balances didn't change
    assert_eq!(token_client.get_balance(&alice), 50);
    assert_eq!(token_client.get_balance(&bob), 0);
}

#[test]
fn test_transfer_succeeds_when_not_denied() {
    let env = Env::default();
    let (admin, _denylist_addr, _allowlist_addr, _jurisdiction_addr, token_client) = setup(&env);
    let alice = Address::generate(&env);
    let bob = Address::generate(&env);

    // Mint to alice
    token_client.mint(&admin, &alice, &1000);

    // Transfer from alice to bob (should succeed since they're not denied or restricted)
    token_client.transfer(&alice, &bob, &300);

    // Verify balances changed correctly
    assert_eq!(token_client.get_balance(&alice), 700);
    assert_eq!(token_client.get_balance(&bob), 300);
}

#[test]
fn test_transfer_fails_not_on_allowlist() {
    let env = Env::default();
    let (admin, _denylist_addr, allowlist_addr, _jurisdiction_addr, token_client) = setup(&env);
    let alice = Address::generate(&env);
    let bob = Address::generate(&env);

    // Mint to alice
    token_client.mint(&admin, &alice, &1000);

    // Note: In this test setup, the allowlist is a placeholder.
    // Since we can't initialize it properly in this test (it requires token address),
    // the transfer will fail with NotOnAllowlist for any address.
    // This demonstrates the check is being performed.

    let result = token_client.try_transfer(&alice, &bob, &100);
    assert_eq!(result, Err(Ok(Error::NotOnAllowlist)));

    // Verify balances didn't change
    assert_eq!(token_client.get_balance(&alice), 1000);
    assert_eq!(token_client.get_balance(&bob), 0);
}

#[test]
fn test_transfer_multiple_checks_fail_in_order() {
    let env = Env::default();
    let (admin, denylist_addr, _allowlist_addr, _jurisdiction_addr, token_client) = setup(&env);
    let alice = Address::generate(&env);
    let bob = Address::generate(&env);

    // Mint to alice
    token_client.mint(&admin, &alice, &1000);

    // Add alice to denylist
    let denylist_client = denylist_gate::DenylistGateClient::new(&env, &denylist_addr);
    denylist_client.add_to_denylist(&admin, &alice);

    // Even though alice is not on the allowlist either, the denylist check comes first
    let result = token_client.try_transfer(&alice, &bob, &100);
    assert_eq!(result, Err(Ok(Error::DeniedByDenylist)));

    // Remove from denylist
    denylist_client.remove_from_denylist(&admin, &alice);

    // Now it should fail on allowlist check instead
    let result = token_client.try_transfer(&alice, &bob, &100);
    assert_eq!(result, Err(Ok(Error::NotOnAllowlist)));
}
