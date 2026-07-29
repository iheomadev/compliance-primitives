use super::*;
use soroban_sdk::testutils::{Address as _, Events as _};
use soroban_sdk::{contract, contractimpl, symbol_short, vec, Env, IntoVal, Map, Symbol, Val};

/// A minimal token double used only by these tests, so `allowlist-token`'s
/// unit tests don't depend on any particular real SEP-41 implementation.
#[contract]
struct MockToken;

#[contractimpl]
impl MockToken {
    pub fn transfer(env: Env, from: Address, to: Address, amount: i128) {
        from.require_auth();
        env.storage()
            .instance()
            .set(&Symbol::new(&env, "last"), &(from, to, amount));
    }

    pub fn last_transfer(env: Env) -> Option<(Address, Address, i128)> {
        env.storage().instance().get(&Symbol::new(&env, "last"))
    }
}

fn setup(env: &Env) -> (Address, Address, Address, AllowlistTokenClient<'_>) {
    env.mock_all_auths();
    let admin = Address::generate(env);
    let token_id = env.register(MockToken, ());
    let contract_id = env.register(AllowlistToken, ());
    let client = AllowlistTokenClient::new(env, &contract_id);
    client.initialize(&admin, &token_id);
    (admin, token_id, contract_id, client)
}

#[test]
fn test_initialize_and_allowlist_roundtrip() {
    let env = Env::default();
    let (admin, _token_id, _contract_id, client) = setup(&env);
    let alice = Address::generate(&env);

    assert!(!client.is_allowed(&alice));
    client.add_to_allowlist(&admin, &alice);
    assert!(client.is_allowed(&alice));
    client.remove_from_allowlist(&admin, &alice);
    assert!(!client.is_allowed(&alice));
}

#[test]
fn test_transfer_forwards_to_underlying_token_when_both_allowlisted() {
    let env = Env::default();
    let (admin, token_id, _contract_id, client) = setup(&env);
    let alice = Address::generate(&env);
    let bob = Address::generate(&env);

    client.add_to_allowlist(&admin, &alice);
    client.add_to_allowlist(&admin, &bob);

    let ok = client.transfer(&alice, &bob, &500);
    assert!(ok);

    let token_client = MockTokenClient::new(&env, &token_id);
    let last = token_client.last_transfer().unwrap();
    assert_eq!(last, (alice, bob, 500));
}

#[test]
fn test_transfer_blocked_when_recipient_not_allowlisted() {
    let env = Env::default();
    let (admin, _token_id, contract_id, client) = setup(&env);
    let alice = Address::generate(&env);
    let bob = Address::generate(&env);

    client.add_to_allowlist(&admin, &alice);

    let ok = client.transfer(&alice, &bob, &500);
    assert!(!ok);

    assert_eq!(
        env.events().all(),
        vec![
            &env,
            (
                contract_id.clone(),
                (symbol_short!("blocked"), alice.clone(), bob.clone()).into_val(&env),
                Map::<Symbol, Val>::from_array(
                    &env,
                    [(symbol_short!("amount"), 500i128.into_val(&env))]
                )
                .into_val(&env),
            ),
        ]
    );
}

#[test]
fn test_add_to_allowlist_rejects_non_admin() {
    let env = Env::default();
    let (_admin, _token_id, _contract_id, client) = setup(&env);
    let impostor = Address::generate(&env);
    let alice = Address::generate(&env);

    let result = client.try_add_to_allowlist(&impostor, &alice);
    assert_eq!(result, Err(Ok(Error::NotAuthorized)));
    assert!(!client.is_allowed(&alice));
}

#[test]
fn test_non_admin_allowlist_mutations_rejected_end_to_end() {
    let env = Env::default();
    let (admin, _token_id, _contract_id, client) = setup(&env);
    let impostor = Address::generate(&env);
    let alice = Address::generate(&env);

    let add_result = client.try_add_to_allowlist(&impostor, &alice);
    assert_eq!(add_result, Err(Ok(Error::NotAuthorized)));
    assert!(!client.is_allowed(&alice));

    client.add_to_allowlist(&admin, &alice);
    assert!(client.is_allowed(&alice));

    let remove_result = client.try_remove_from_allowlist(&impostor, &alice);
    assert_eq!(remove_result, Err(Ok(Error::NotAuthorized)));
    assert!(client.is_allowed(&alice));
}

#[test]
fn test_remove_from_allowlist_never_added_is_noop() {
    let env = Env::default();
    let (admin, _token_id, contract_id, client) = setup(&env);
    let never_added = Address::generate(&env);

    assert!(!client.is_allowed(&never_added));

    client.remove_from_allowlist(&admin, &never_added);

    assert_eq!(
        env.events().all(),
        vec![
            &env,
            (
                contract_id.clone(),
                (Symbol::new(&env, "allow_remove"), never_added.clone()).into_val(&env),
                Map::<Symbol, Val>::new(&env).into_val(&env),
            ),
        ]
    );
    assert!(!client.is_allowed(&never_added));
}

#[test]
fn test_is_allowed_false_before_initialize() {
    let env = Env::default();
    let contract_id = env.register(AllowlistToken, ());
    let client = AllowlistTokenClient::new(&env, &contract_id);
    let alice = Address::generate(&env);

    assert!(!client.is_allowed(&alice));
}

#[test]
fn test_double_initialize_fails() {
    let env = Env::default();
    let (admin, token_id, _contract_id, client) = setup(&env);
    let result = client.try_initialize(&admin, &token_id);
    assert_eq!(result, Err(Ok(Error::AlreadyInitialized)));
}

#[test]
fn test_add_to_allowlist_emits_allow_add_event() {
    let env = Env::default();
    let (admin, _token_id, contract_id, client) = setup(&env);
    let alice = Address::generate(&env);

    client.add_to_allowlist(&admin, &alice);

    assert_eq!(
        env.events().all(),
        vec![
            &env,
            (
                contract_id.clone(),
                (Symbol::new(&env, "allow_add"), alice.clone()).into_val(&env),
                Map::<Symbol, Val>::new(&env).into_val(&env),
            ),
        ]
    );
}

#[test]
fn test_remove_from_allowlist_emits_allow_remove_event() {
    let env = Env::default();
    let (admin, _token_id, contract_id, client) = setup(&env);
    let alice = Address::generate(&env);
    client.add_to_allowlist(&admin, &alice);

    client.remove_from_allowlist(&admin, &alice);

    assert_eq!(
        env.events().all(),
        vec![
            &env,
            (
                contract_id.clone(),
                (Symbol::new(&env, "allow_remove"), alice.clone()).into_val(&env),
                Map::<Symbol, Val>::new(&env).into_val(&env),
            ),
        ]
    );
}

#[test]
fn test_add_to_allowlist_extends_persistent_ttl() {
    use soroban_sdk::testutils::storage::Persistent as _;
    use soroban_sdk::testutils::Ledger;

    let env = Env::default();
    env.ledger().with_mut(|li| {
        li.sequence_number = 100_000;
        li.min_persistent_entry_ttl = 500;
        li.max_entry_ttl = 6_311_520;
    });

    let (admin, _token_id, contract_id, client) = setup(&env);
    let alice = Address::generate(&env);

    client.add_to_allowlist(&admin, &alice);

    env.as_contract(&contract_id, || {
        let ttl = env
            .storage()
            .persistent()
            .get_ttl(&DataKey::Allowed(alice.clone()));
        assert_eq!(
            ttl, ALLOWED_TTL_EXTEND_TO,
            "fresh allowlist write should bump TTL to ALLOWED_TTL_EXTEND_TO"
        );
    });

    // Advance far enough that remaining TTL falls below the threshold, then
    // re-add and confirm extension runs again.
    env.ledger().with_mut(|li| {
        li.sequence_number += ALLOWED_TTL_EXTEND_TO - ALLOWED_TTL_THRESHOLD + 1;
    });
    env.as_contract(&contract_id, || {
        let ttl = env
            .storage()
            .persistent()
            .get_ttl(&DataKey::Allowed(alice.clone()));
        assert!(
            ttl < ALLOWED_TTL_THRESHOLD,
            "TTL should be below threshold after ledger bump, got {ttl}"
        );
    });

    client.add_to_allowlist(&admin, &alice);
    env.as_contract(&contract_id, || {
        let ttl = env
            .storage()
            .persistent()
            .get_ttl(&DataKey::Allowed(alice.clone()));
        assert_eq!(ttl, ALLOWED_TTL_EXTEND_TO);
    });
}

/// Property: after any sequence of add/remove on a fixed address pool,
/// `is_allowed(addr)` matches whether the last op touching `addr` was an add.
#[test]
fn prop_allowlist_add_remove_last_write_wins() {
    use proptest::prelude::*;
    use proptest::test_runner::{Config, TestRunner};

    #[derive(Clone, Debug)]
    enum Op {
        Add(usize),
        Remove(usize),
    }

    let mut runner = TestRunner::new(Config {
        cases: 64,
        max_shrink_iters: 100,
        ..Config::default()
    });

    runner
        .run(
            &(1usize..32).prop_flat_map(|len| {
                proptest::collection::vec(
                    (0usize..4).prop_flat_map(|addr_i| {
                        proptest::bool::ANY.prop_map(move |is_add| {
                            if is_add {
                                Op::Add(addr_i)
                            } else {
                                Op::Remove(addr_i)
                            }
                        })
                    }),
                    len,
                )
            }),
            |ops| {
                let env = Env::default();
                let (admin, _token_id, _contract_id, client) = setup(&env);
                let addresses: [Address; 4] = [
                    Address::generate(&env),
                    Address::generate(&env),
                    Address::generate(&env),
                    Address::generate(&env),
                ];
                let mut model = [false; 4];

                for op in &ops {
                    match *op {
                        Op::Add(i) => {
                            client.add_to_allowlist(&admin, &addresses[i]);
                            model[i] = true;
                        }
                        Op::Remove(i) => {
                            client.remove_from_allowlist(&admin, &addresses[i]);
                            model[i] = false;
                        }
                    }
                }

                for (i, address) in addresses.iter().enumerate() {
                    prop_assert_eq!(
                        client.is_allowed(address),
                        model[i],
                        "addr {} mismatch after {:?}",
                        i,
                        ops
                    );
                }
                Ok(())
            },
        )
        .unwrap();
}
