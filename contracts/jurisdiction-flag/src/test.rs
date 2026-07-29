use super::*;
use soroban_sdk::testutils::{Address as _, Events as _};
use soroban_sdk::{vec, Env};

fn setup(env: &Env) -> (Address, Address, JurisdictionFlagClient<'_>) {
    env.mock_all_auths();
    let issuer = Address::generate(env);
    let contract_id = env.register(JurisdictionFlag, ());
    let client = JurisdictionFlagClient::new(env, &contract_id);
    client.initialize(&issuer);
    (issuer, contract_id, client)
}

#[test]
fn test_set_and_get_jurisdiction() {
    let env = Env::default();
    let (issuer, _contract_id, client) = setup(&env);
    let alice = Address::generate(&env);

    assert_eq!(client.get_jurisdiction(&alice), None);

    let code = String::from_str(&env, "US");
    client.set_jurisdiction(&issuer, &alice, &code);
    assert_eq!(client.get_jurisdiction(&alice), Some(code));
}

#[test]
fn test_set_jurisdiction_rejects_non_issuer() {
    let env = Env::default();
    let (_issuer, _contract_id, client) = setup(&env);
    let impostor = Address::generate(&env);
    let alice = Address::generate(&env);
    let code = String::from_str(&env, "US");

    let result = client.try_set_jurisdiction(&impostor, &alice, &code);
    assert_eq!(result, Err(Ok(Error::NotAuthorized)));
    assert_eq!(client.get_jurisdiction(&alice), None);
}

#[test]
fn test_remove_jurisdiction_multiple_clears_mixed_addresses() {
    let env = Env::default();
    let (issuer, _contract_id, client) = setup(&env);
    let alice = Address::generate(&env);
    let bob = Address::generate(&env);
    let never_set = Address::generate(&env);

    client.set_jurisdiction(&issuer, &alice, &String::from_str(&env, "US"));
    client.set_jurisdiction(&issuer, &bob, &String::from_str(&env, "CA"));
    assert_eq!(
        client.get_jurisdiction(&alice),
        Some(String::from_str(&env, "US"))
    );
    assert_eq!(
        client.get_jurisdiction(&bob),
        Some(String::from_str(&env, "CA"))
    );
    assert_eq!(client.get_jurisdiction(&never_set), None);

    let addresses = vec![&env, alice.clone(), bob.clone(), never_set.clone()];
    client.remove_jurisdiction_multiple(&issuer, &addresses);

    assert_eq!(client.get_jurisdiction(&alice), None);
    assert_eq!(client.get_jurisdiction(&bob), None);
    assert_eq!(client.get_jurisdiction(&never_set), None);
}

#[test]
fn test_remove_jurisdiction_multiple_rejects_non_issuer() {
    let env = Env::default();
    let (issuer, _contract_id, client) = setup(&env);
    let impostor = Address::generate(&env);
    let alice = Address::generate(&env);

    client.set_jurisdiction(&issuer, &alice, &String::from_str(&env, "US"));

    let addresses = vec![&env, alice.clone()];
    let result = client.try_remove_jurisdiction_multiple(&impostor, &addresses);
    assert_eq!(result, Err(Ok(Error::NotAuthorized)));
    assert_eq!(
        client.get_jurisdiction(&alice),
        Some(String::from_str(&env, "US"))
    );
}

#[test]
fn test_remove_jurisdiction_multiple_empty_vec_is_noop() {
    let env = Env::default();
    let (issuer, _contract_id, client) = setup(&env);
    let alice = Address::generate(&env);
    client.set_jurisdiction(&issuer, &alice, &String::from_str(&env, "US"));

    let empty: Vec<Address> = vec![&env];
    client.remove_jurisdiction_multiple(&issuer, &empty);

    assert_eq!(
        client.get_jurisdiction(&alice),
        Some(String::from_str(&env, "US"))
    );
}

#[test]
fn test_is_permitted_jurisdiction_true_when_code_in_list() {
    let env = Env::default();
    let (issuer, _contract_id, client) = setup(&env);
    let alice = Address::generate(&env);
    let code = String::from_str(&env, "US");
    client.set_jurisdiction(&issuer, &alice, &code);

    let allowed = vec![
        &env,
        String::from_str(&env, "CA"),
        String::from_str(&env, "US"),
    ];
    assert!(client.is_permitted_jurisdiction(&alice, &allowed));
}

#[test]
fn test_is_permitted_jurisdiction_false_when_no_jurisdiction_set() {
    let env = Env::default();
    let (_issuer, _contract_id, client) = setup(&env);
    let alice = Address::generate(&env);
    let allowed = vec![&env, String::from_str(&env, "US")];
    assert!(!client.is_permitted_jurisdiction(&alice, &allowed));
}

#[test]
fn test_is_permitted_jurisdiction_false_with_empty_allowed_list() {
    let env = Env::default();
    let (issuer, _contract_id, client) = setup(&env);
    let alice = Address::generate(&env);
    let code = String::from_str(&env, "US");
    client.set_jurisdiction(&issuer, &alice, &code);

    let allowed: Vec<String> = vec![&env];
    assert!(!client.is_permitted_jurisdiction(&alice, &allowed));
}

#[test]
fn test_is_permitted_jurisdiction_false_when_no_jurisdiction_and_empty_allowed_list() {
    let env = Env::default();
    let (_issuer, _contract_id, client) = setup(&env);
    let alice = Address::generate(&env);

    let allowed: Vec<String> = vec![&env];
    assert!(!client.is_permitted_jurisdiction(&alice, &allowed));
}

#[test]
fn test_set_jurisdiction_fails_before_initialize() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(JurisdictionFlag, ());
    let client = JurisdictionFlagClient::new(&env, &contract_id);
    let issuer = Address::generate(&env);
    let alice = Address::generate(&env);
    let code = String::from_str(&env, "US");

    let result = client.try_set_jurisdiction(&issuer, &alice, &code);
    assert_eq!(result, Err(Ok(Error::NotInitialized)));
    assert_eq!(env.events().all(), vec![&env]);
}

#[test]
fn test_double_initialize_fails() {
    let env = Env::default();
    let (issuer, _contract_id, client) = setup(&env);
    let result = client.try_initialize(&issuer);
    assert_eq!(result, Err(Ok(Error::AlreadyInitialized)));
}
