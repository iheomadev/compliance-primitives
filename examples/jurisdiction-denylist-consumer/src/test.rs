use super::*;
use denylist_gate::{DenylistGate, DenylistGateClient};
use jurisdiction_flag::{JurisdictionFlag, JurisdictionFlagClient};
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{vec, Env, String};

fn setup(
    env: &Env,
) -> (
    Address,
    Address,
    Address,
    Address,
    JurisdictionDenylistConsumerClient<'_>,
) {
    env.mock_all_auths();

    let gate_admin = Address::generate(env);
    let gate_id = env.register(DenylistGate, ());
    DenylistGateClient::new(env, &gate_id).initialize(&gate_admin);

    let jurisdiction_issuer = Address::generate(env);
    let jurisdiction_id = env.register(JurisdictionFlag, ());
    JurisdictionFlagClient::new(env, &jurisdiction_id).initialize(&jurisdiction_issuer);

    let allowed = vec![env, String::from_str(env, "US"), String::from_str(env, "CA")];

    let token_id = env.register(JurisdictionDenylistConsumer, ());
    let client = JurisdictionDenylistConsumerClient::new(env, &token_id);
    client.initialize(&gate_id, &jurisdiction_id, &allowed);

    (
        gate_admin,
        gate_id,
        jurisdiction_issuer,
        jurisdiction_id,
        client,
    )
}

#[test]
fn test_transfer_succeeds_when_both_checks_pass() {
    let env = Env::default();
    let (_gate_admin, _gate_id, jurisdiction_issuer, jurisdiction_id, client) = setup(&env);
    let alice = Address::generate(&env);
    let bob = Address::generate(&env);

    let j_client = JurisdictionFlagClient::new(&env, &jurisdiction_id);
    j_client.set_jurisdiction(&jurisdiction_issuer, &alice, &String::from_str(&env, "US"));

    client.mint(&alice, &1_000);
    client.transfer(&alice, &bob, &400);

    assert_eq!(client.balance(&alice), 600);
    assert_eq!(client.balance(&bob), 400);
}

#[test]
fn test_transfer_blocked_when_denylist_check_fails() {
    let env = Env::default();
    let (gate_admin, gate_id, jurisdiction_issuer, jurisdiction_id, client) = setup(&env);
    let alice = Address::generate(&env);
    let bob = Address::generate(&env);

    let j_client = JurisdictionFlagClient::new(&env, &jurisdiction_id);
    j_client.set_jurisdiction(&jurisdiction_issuer, &alice, &String::from_str(&env, "US"));

    client.mint(&alice, &1_000);
    DenylistGateClient::new(&env, &gate_id).add_to_denylist(&gate_admin, &alice);

    let result = client.try_transfer(&alice, &bob, &400);
    assert_eq!(result, Err(Ok(Error::DeniedByGate)));
    assert_eq!(client.balance(&alice), 1_000);
    assert_eq!(client.balance(&bob), 0);
}

#[test]
fn test_transfer_blocked_when_jurisdiction_check_fails() {
    let env = Env::default();
    let (_gate_admin, _gate_id, jurisdiction_issuer, jurisdiction_id, client) = setup(&env);
    let alice = Address::generate(&env);
    let bob = Address::generate(&env);

    let j_client = JurisdictionFlagClient::new(&env, &jurisdiction_id);
    // Set alice to a non-permitted jurisdiction ("UK")
    j_client.set_jurisdiction(&jurisdiction_issuer, &alice, &String::from_str(&env, "UK"));

    client.mint(&alice, &1_000);

    let result = client.try_transfer(&alice, &bob, &400);
    assert_eq!(result, Err(Ok(Error::DeniedByJurisdiction)));
    assert_eq!(client.balance(&alice), 1_000);
    assert_eq!(client.balance(&bob), 0);
}
