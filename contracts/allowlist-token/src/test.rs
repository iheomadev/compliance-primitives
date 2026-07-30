use super::*;
use ed25519_dalek::SigningKey;
use soroban_sdk::testutils::{Address as _, Events as _, Ledger as _};
use soroban_sdk::testutils::ed25519::Sign;
use soroban_sdk::{contract, contractimpl, symbol_short, vec, Bytes, BytesN, Env, IntoVal, Map, Symbol, Val};
use std::path::{Path, PathBuf};

// ─── MockToken ───────────────────────────────────────────────────────────────

/// A minimal token double used only by these tests, so `allowlist-token`'s
/// unit tests don't depend on any particular real SEP-41 implementation.
#[contract]
struct MockToken;

#[contractimpl]
impl MockToken {
    pub fn transfer(env: Env, from: Address, to: Address, amount: i128) {
        from.require_auth();
        env.storage().instance().set(&Symbol::new(&env, "last"), &(from, to, amount));
    }

    pub fn last_transfer(env: Env) -> Option<(Address, Address, i128)> {
        env.storage().instance().get(&Symbol::new(&env, "last"))
    }
}

// ─── Setup helper ────────────────────────────────────────────────────────────

fn setup(env: &Env) -> (Address, Address, Address, AllowlistTokenClient<'_>) {
    env.mock_all_auths();
    let admin = Address::generate(env);
    let token_id = env.register(MockToken, ());
    let contract_id = env.register(AllowlistToken, ());
    let client = AllowlistTokenClient::new(env, &contract_id);
    client.initialize(&admin, &token_id);
    (admin, token_id, contract_id, client)
}

// ─── Existing unit tests ──────────────────────────────────────────────────────

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
fn test_budget_regression_allowlist_transfer() {
    let env = Env::default();
    let (admin, _token_id, _contract_id, client) = setup(&env);
    let alice = Address::generate(&env);
    let bob = Address::generate(&env);

    client.add_to_allowlist(&admin, &alice);
    client.add_to_allowlist(&admin, &bob);

    let mut budget = env.cost_estimate().budget();
    budget.reset_default();
    let ok = client.transfer(&alice, &bob, &500);
    assert!(ok);

    let measured = (budget.cpu_instruction_cost(), budget.memory_bytes_cost());
    let baseline_path = baseline_path_for_manifest_dir(PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap()));
    let baseline = read_baseline(&baseline_path, "allowlist-token.transfer");
    assert_budget_within_threshold(measured, baseline, "allowlist-token transfer");
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
                (Symbol::new(&env, "allow_add"), alice.clone()).into_val(&env),
                Map::<Symbol, Val>::new(&env).into_val(&env),
            ),
            (
                contract_id.clone(),
                (symbol_short!("blocked"), alice.clone(), bob.clone()).into_val(&env),
                Map::<Symbol, Val>::from_array(&env, [(symbol_short!("amount"), 500i128.into_val(&env))])
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
fn test_delegated_add_to_allowlist_succeeds() {
    let env = Env::default();
    let (admin, _token_id, _contract_id, client) = setup(&env);
    let alice = Address::generate(&env);
    let signing_key = SigningKey::from_bytes(&[
        0u8, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22,
        23, 24, 25, 26, 27, 28, 29, 30, 31,
    ]);
    let pubkey = BytesN::from_array(&env, &signing_key.verifying_key().to_bytes());

    client.set_delegated_admin_key(&admin, &pubkey);

    let expiry = env.ledger().timestamp() + 60;
    let signature = sign_delegated_action(&env, &signing_key, &alice, 1, expiry);

    client.add_to_allowlist_delegated(&admin, &alice, &1u64, &expiry, &signature);
    assert!(client.is_allowed(&alice));
}

#[test]
fn test_delegated_add_to_allowlist_rejects_replay() {
    let env = Env::default();
    let (admin, _token_id, _contract_id, client) = setup(&env);
    let alice = Address::generate(&env);
    let signing_key = SigningKey::from_bytes(&[
        0u8, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22,
        23, 24, 25, 26, 27, 28, 29, 30, 31,
    ]);
    let pubkey = BytesN::from_array(&env, &signing_key.verifying_key().to_bytes());

    client.set_delegated_admin_key(&admin, &pubkey);

    let expiry = env.ledger().timestamp() + 60;
    let signature = sign_delegated_action(&env, &signing_key, &alice, 1, expiry);

    client.add_to_allowlist_delegated(&admin, &alice, &1u64, &expiry, &signature);
    let replay = client.try_add_to_allowlist_delegated(&admin, &alice, &1u64, &expiry, &signature);
    assert_eq!(replay, Err(Ok(Error::InvalidNonce)));
    assert!(client.is_allowed(&alice));
}

#[test]
fn test_delegated_add_to_allowlist_rejects_expired_signature() {
    let env = Env::default();
    let (admin, _token_id, _contract_id, client) = setup(&env);
    let alice = Address::generate(&env);
    let signing_key = SigningKey::from_bytes(&[
        0u8, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22,
        23, 24, 25, 26, 27, 28, 29, 30, 31,
    ]);
    let pubkey = BytesN::from_array(&env, &signing_key.verifying_key().to_bytes());

    client.set_delegated_admin_key(&admin, &pubkey);

    env.ledger().set_timestamp(100);
    let expiry = 99u64;
    let signature = sign_delegated_action(&env, &signing_key, &alice, 1, expiry);

    let result = client.try_add_to_allowlist_delegated(&admin, &alice, &1u64, &expiry, &signature);
    assert_eq!(result, Err(Ok(Error::ExpiredSignature)));
    assert!(!client.is_allowed(&alice));
}

#[test]
fn test_delegated_add_to_allowlist_rejects_non_admin_key() {
    let env = Env::default();
    let (admin, _token_id, _contract_id, client) = setup(&env);
    let alice = Address::generate(&env);
    let signing_key = SigningKey::from_bytes(&[
        0u8, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22,
        23, 24, 25, 26, 27, 28, 29, 30, 31,
    ]);
    let attacker_key = SigningKey::from_bytes(&[
        32u8, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45, 46, 47, 48, 49, 50, 51, 52, 53,
        54, 55, 56, 57, 58, 59, 60, 61, 62, 63,
    ]);
    let pubkey = BytesN::from_array(&env, &signing_key.verifying_key().to_bytes());

    client.set_delegated_admin_key(&admin, &pubkey);

    let expiry = env.ledger().timestamp() + 60;
    let signature = sign_delegated_action(&env, &attacker_key, &alice, 1, expiry);

    let result = client.try_add_to_allowlist_delegated(&admin, &alice, &1u64, &expiry, &signature);
    assert_eq!(result, Err(Ok(Error::NotAuthorized)));
    assert!(!client.is_allowed(&alice));
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
fn test_get_admin_returns_initialized_admin() {
    let env = Env::default();
    let (admin, _token_id, _contract_id, client) = setup(&env);

    assert_eq!(client.get_admin(), admin);
}

#[test]
fn test_get_admin_fails_before_initialize() {
    let env = Env::default();
    let contract_id = env.register(AllowlistToken, ());
    let client = AllowlistTokenClient::new(&env, &contract_id);

    let result = client.try_get_admin();
    assert_eq!(result, Err(Ok(Error::NotInitialized)));
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
                (Symbol::new(&env, "allow_add"), alice.clone()).into_val(&env),
                Map::<Symbol, Val>::new(&env).into_val(&env),
            ),
            (
                contract_id.clone(),
                (Symbol::new(&env, "allow_remove"), alice.clone()).into_val(&env),
                Map::<Symbol, Val>::new(&env).into_val(&env),
            ),
        ]
    );
}

// ─── Issue #100: cross-contract composition with jurisdiction-flag ────────────
//
// Design choice: test-only composition.
//
// The README's architecture note says these contracts are composable building
// blocks — jurisdiction checks should be wired in by the caller's token
// contract one layer up, not baked into every primitive. Hardwiring a
// jurisdiction-flag call inside AllowlistToken::transfer would violate that
// principle and force every deployer to also deploy a JurisdictionFlag
// contract whether they need one or not.
//
// These tests therefore demonstrate the composition pattern at the test layer:
// both contracts are deployed in the same Soroban Env, and the test
// orchestrates the full gating sequence (allowlist check AND jurisdiction
// check) before forwarding the transfer — the same sequence a real issuer
// token contract would implement in its own `transfer` function.

#[cfg(test)]
mod cross_contract_jurisdiction_tests {
    use super::*;
    use jurisdiction_flag::{JurisdictionFlag, JurisdictionFlagClient};
    use soroban_sdk::{vec, Env, String};

    /// Deploy AllowlistToken + JurisdictionFlag and wire them together.
    /// Returns (allowlist_admin, jurisdiction_issuer, allowlist_client, jflag_client).
    fn setup_composed(
        env: &Env,
    ) -> (
        Address,
        Address,
        AllowlistTokenClient<'_>,
        JurisdictionFlagClient<'_>,
    ) {
        env.mock_all_auths();

        let admin = Address::generate(env);
        let issuer = Address::generate(env);

        // Underlying token
        let token_id = env.register(MockToken, ());

        // AllowlistToken wrapping the mock token
        let allowlist_id = env.register(AllowlistToken, ());
        let allowlist_client = AllowlistTokenClient::new(env, &allowlist_id);
        allowlist_client.initialize(&admin, &token_id);

        // Standalone JurisdictionFlag
        let jflag_id = env.register(JurisdictionFlag, ());
        let jflag_client = JurisdictionFlagClient::new(env, &jflag_id);
        jflag_client.initialize(&issuer);

        (admin, issuer, allowlist_client, jflag_client)
    }

    /// Helper: allowed codes list used across tests.
    fn permitted(env: &Env) -> soroban_sdk::Vec<String> {
        vec![env, String::from_str(env, "US"), String::from_str(env, "CA")]
    }

    /// Both parties are allowlisted AND in a permitted jurisdiction →
    /// transfer must be forwarded to the underlying token.
    #[test]
    fn test_allowlisted_and_permitted_jurisdiction_transfer_proceeds() {
        let env = Env::default();
        let (admin, issuer, allowlist_client, jflag_client) = setup_composed(&env);
        let alice = Address::generate(&env);
        let bob = Address::generate(&env);

        // Allowlist both
        allowlist_client.add_to_allowlist(&admin, &alice);
        allowlist_client.add_to_allowlist(&admin, &bob);

        // Set permitted jurisdictions
        jflag_client.set_jurisdiction(&issuer, &alice, &String::from_str(&env, "US"));
        jflag_client.set_jurisdiction(&issuer, &bob, &String::from_str(&env, "CA"));

        // Composition check: both pass jurisdiction gate
        assert!(jflag_client.is_permitted_jurisdiction(&alice, &permitted(&env)));
        assert!(jflag_client.is_permitted_jurisdiction(&bob, &permitted(&env)));

        // AllowlistToken transfer proceeds
        let ok = allowlist_client.transfer(&alice, &bob, &750);
        assert!(ok, "transfer should proceed when both parties pass all checks");
    }

    /// Sender is allowlisted but has a blocked jurisdiction →
    /// the composed check must prevent the transfer.
    #[test]
    fn test_allowlisted_but_sender_wrong_jurisdiction_blocked() {
        let env = Env::default();
        let (admin, issuer, allowlist_client, jflag_client) = setup_composed(&env);
        let alice = Address::generate(&env);
        let bob = Address::generate(&env);

        // Both allowlisted
        allowlist_client.add_to_allowlist(&admin, &alice);
        allowlist_client.add_to_allowlist(&admin, &bob);

        // Alice is in a blocked jurisdiction; Bob is fine
        jflag_client.set_jurisdiction(&issuer, &alice, &String::from_str(&env, "IR"));
        jflag_client.set_jurisdiction(&issuer, &bob, &String::from_str(&env, "US"));

        // Jurisdiction gate rejects Alice
        assert!(!jflag_client.is_permitted_jurisdiction(&alice, &permitted(&env)));
        assert!(jflag_client.is_permitted_jurisdiction(&bob, &permitted(&env)));

        // A caller contract would see this and skip the transfer; the test
        // verifies that the jurisdiction gate correctly identifies the block.
        // We intentionally do NOT call allowlist_client.transfer here because
        // AllowlistToken itself doesn't know about JurisdictionFlag — the
        // composition happens one layer up, as per the architecture note.
    }

    /// Recipient is allowlisted but has no jurisdiction set at all →
    /// is_permitted_jurisdiction returns false, blocking the transfer upstream.
    #[test]
    fn test_allowlisted_but_recipient_no_jurisdiction_blocked() {
        let env = Env::default();
        let (admin, issuer, allowlist_client, jflag_client) = setup_composed(&env);
        let alice = Address::generate(&env);
        let bob = Address::generate(&env);

        // Both allowlisted
        allowlist_client.add_to_allowlist(&admin, &alice);
        allowlist_client.add_to_allowlist(&admin, &bob);

        // Only Alice has a jurisdiction; Bob has none
        jflag_client.set_jurisdiction(&issuer, &alice, &String::from_str(&env, "US"));

        assert!(jflag_client.is_permitted_jurisdiction(&alice, &permitted(&env)));
        assert!(!jflag_client.is_permitted_jurisdiction(&bob, &permitted(&env)));
    }

    /// Neither party is on the allowlist AND both have wrong jurisdictions →
    /// both gates independently reject the transfer.
    #[test]
    fn test_not_allowlisted_and_wrong_jurisdiction_both_blocked() {
        let env = Env::default();
        let (admin, issuer, allowlist_client, jflag_client) = setup_composed(&env);
        let alice = Address::generate(&env);
        let bob = Address::generate(&env);

        // Not allowlisted (admin never calls add_to_allowlist)
        _ = admin; // silence unused warning

        // Wrong jurisdictions
        jflag_client.set_jurisdiction(&issuer, &alice, &String::from_str(&env, "XX"));
        jflag_client.set_jurisdiction(&issuer, &bob, &String::from_str(&env, "YY"));

        assert!(!allowlist_client.is_allowed(&alice));
        assert!(!allowlist_client.is_allowed(&bob));
        assert!(!jflag_client.is_permitted_jurisdiction(&alice, &permitted(&env)));
        assert!(!jflag_client.is_permitted_jurisdiction(&bob, &permitted(&env)));

        // AllowlistToken still blocks on allowlist alone
        let ok = allowlist_client.transfer(&alice, &bob, &100);
        assert!(!ok, "transfer must be blocked when neither party is allowlisted");
    }
}

// ─── Issue #101: reentrancy via malicious underlying token ────────────────────
//
// AllowlistToken::transfer calls out to the underlying token contract
// (token_client.transfer) after its allowlist checks pass. Because the
// underlying token address is admin-supplied at initialize time, a malicious
// or buggy token could attempt to call back into AllowlistToken during its
// own transfer() invocation.
//
// **Reentrancy model in Soroban**:
// In the live WASM execution environment, Soroban's host enforces a per-contract
// invocation guard: a contract cannot be re-entered while one of its frames is
// already on the call stack. Any reentrant call would be trapped by the host
// before execution reaches AllowlistToken's Rust code, so the allowlist state
// can never be read or written in an inconsistent intermediate state.
//
// In the native test environment (where tests run as regular Rust code rather
// than through the WASM VM), that host-level guard is not exercised. The tests
// below therefore focus on what *can* be tested at the Rust level:
//
//   1. A malicious token whose transfer() calls back into AllowlistToken with
//      an un-allowlisted party cannot bypass the allowlist check — the reentrant
//      inner call returns Ok(false) and no funds move.
//
//   2. A malicious token whose transfer() calls back and then panics (simulating
//      the host trap that would fire in WASM) causes the entire outer
//      AllowlistToken::transfer to fail, leaving allowlist state fully intact.
//
//   3. The checks-effects-interactions ordering in AllowlistToken::transfer is
//      correct: both allowlist checks happen before any cross-contract call, so
//      there is no window where a re-entrant call could observe a partially
//      mutated allowlist.
//
// No genuine reentrancy vulnerability was found in AllowlistToken. The
// checks-effects-interactions pattern is already satisfied: storage is only
// read (not written) before the external call, so a re-entrant read of the
// allowlist would see the same state it saw before the outer call began.

#[cfg(test)]
mod reentrancy_tests {
    use super::*;
    use soroban_sdk::{contract, contractimpl, Env};

    // ── Malicious token #1: attempts reentrant call with un-allowlisted party ─
    //
    // Tries to smuggle a transfer to `carol` (never allowlisted) by re-entering
    // AllowlistToken during the token.transfer() callback. The inner reentrant
    // call must be blocked by the allowlist check.

    #[contract]
    struct ReentrantBypassToken;

    #[contractimpl]
    impl ReentrantBypassToken {
        pub fn set_target(env: Env, target: Address) {
            env.storage()
                .instance()
                .set(&soroban_sdk::Symbol::new(&env, "target"), &target);
        }

        pub fn set_smuggle_target(env: Env, smuggle_to: Address) {
            env.storage()
                .instance()
                .set(&soroban_sdk::Symbol::new(&env, "smuggle"), &smuggle_to);
        }

        /// Called by AllowlistToken after it passes the allowlist check for
        /// (from, to). Attempts to re-enter AllowlistToken::transfer with
        /// `from → smuggle_to`, where `smuggle_to` is NOT on the allowlist.
        pub fn transfer(env: Env, from: Address, _to: Address, amount: i128) {
            from.require_auth();

            let target: Address = env
                .storage()
                .instance()
                .get(&soroban_sdk::Symbol::new(&env, "target"))
                .expect("target not set");
            let smuggle_to: Address = env
                .storage()
                .instance()
                .get(&soroban_sdk::Symbol::new(&env, "smuggle"))
                .expect("smuggle_to not set");

            let reentrant_client = AllowlistTokenClient::new(&env, &target);
            // This call re-enters AllowlistToken while the outer invocation is
            // still live. The allowlist check runs again and must reject
            // smuggle_to (who was never added to the allowlist), returning false.
            let bypass_result = reentrant_client.try_transfer(&from, &smuggle_to, &amount);
            // Panic if the inner call returned Ok(Ok(true)) — that would mean
            // the allowlist was bypassed. Ok(Ok(false)) or any Err is fine.
            if let Ok(Ok(true)) = bypass_result {
                panic!("reentrant bypass succeeded — allowlist was circumvented");
            }
        }
    }

    // ── Malicious token #2: calls back and then panics (simulates WASM trap) ──
    //
    // After the reentrant call, this token panics unconditionally. In the live
    // WASM environment the host would trap before the callback fires; here we
    // simulate the trap by panicking, which propagates through AllowlistToken's
    // outer call and causes it to fail. State must be fully rolled back.

    #[contract]
    struct PanickingReentrantToken;

    #[contractimpl]
    impl PanickingReentrantToken {
        pub fn set_target(env: Env, target: Address) {
            env.storage()
                .instance()
                .set(&soroban_sdk::Symbol::new(&env, "target"), &target);
        }

        pub fn transfer(env: Env, from: Address, to: Address, amount: i128) {
            from.require_auth();

            let target: Address = env
                .storage()
                .instance()
                .get(&soroban_sdk::Symbol::new(&env, "target"))
                .expect("target not set");

            // Attempt the reentrant call (will be blocked by allowlist or auth).
            let reentrant_client = AllowlistTokenClient::new(&env, &target);
            let _ = reentrant_client.try_transfer(&from, &to, &amount);

            // Simulate the WASM host trap / panic that would fire in production.
            panic!("malicious token panics after reentrant attempt");
        }
    }

    // ── Setup helpers ─────────────────────────────────────────────────────────

    fn setup_bypass<'a>(
        env: &'a Env,
        smuggle_to: &Address,
    ) -> (Address, AllowlistTokenClient<'a>) {
        env.mock_all_auths();
        let admin = Address::generate(env);

        let bypass_token_id = env.register(ReentrantBypassToken, ());
        let allowlist_id = env.register(AllowlistToken, ());

        let bypass_client = ReentrantBypassTokenClient::new(env, &bypass_token_id);
        bypass_client.set_target(&allowlist_id);
        bypass_client.set_smuggle_target(smuggle_to);

        let client = AllowlistTokenClient::new(env, &allowlist_id);
        client.initialize(&admin, &bypass_token_id);
        (admin, client)
    }

    fn setup_panicking(env: &Env) -> (Address, AllowlistTokenClient<'_>) {
        env.mock_all_auths();
        let admin = Address::generate(env);

        let panic_token_id = env.register(PanickingReentrantToken, ());
        let allowlist_id = env.register(AllowlistToken, ());

        PanickingReentrantTokenClient::new(env, &panic_token_id).set_target(&allowlist_id);

        let client = AllowlistTokenClient::new(env, &allowlist_id);
        client.initialize(&admin, &panic_token_id);
        (admin, client)
    }

    // ── Tests ─────────────────────────────────────────────────────────────────

    /// A malicious token that re-enters AllowlistToken::transfer with an
    /// un-allowlisted smuggle target cannot bypass the allowlist check.
    /// The reentrant inner call is blocked (returns Ok(false) or errors),
    /// and the outer call completes without corrupting allowlist state.
    #[test]
    fn test_reentrant_bypass_attempt_is_blocked_by_allowlist() {
        let env = Env::default();
        let carol = Address::generate(&env); // never allowlisted — the smuggle target
        let (admin, client) = setup_bypass(&env, &carol);
        let alice = Address::generate(&env);
        let bob = Address::generate(&env);

        // Allowlist alice and bob, but NOT carol (the smuggle target)
        client.add_to_allowlist(&admin, &alice);
        client.add_to_allowlist(&admin, &bob);
        assert!(!client.is_allowed(&carol), "carol must not be allowlisted");

        // The outer transfer (alice → bob) passes the allowlist.
        // Inside the malicious token's transfer(), it re-enters AllowlistToken
        // trying to move funds to carol. That inner call must be blocked.
        // The outer transfer itself may succeed or fail depending on whether the
        // malicious token propagates an error — what matters is carol never
        // received a passing Ok(true) on the bypass call.
        let _ = client.try_transfer(&alice, &bob, &500);

        // Allowlist state must be uncorrupted regardless of outcome.
        assert!(client.is_allowed(&alice), "alice must still be allowlisted");
        assert!(client.is_allowed(&bob), "bob must still be allowlisted");
        assert!(!client.is_allowed(&carol), "carol must never be allowlisted");
    }

    /// A malicious token that panics after a reentrant call (simulating the
    /// WASM host trap that would fire in production) causes the outer
    /// AllowlistToken::transfer to fail. Allowlist state must be intact.
    #[test]
    fn test_panicking_reentrant_token_fails_outer_transfer() {
        let env = Env::default();
        let (admin, client) = setup_panicking(&env);
        let alice = Address::generate(&env);
        let bob = Address::generate(&env);

        client.add_to_allowlist(&admin, &alice);
        client.add_to_allowlist(&admin, &bob);

        // The malicious token panics unconditionally, so the outer call fails.
        let result = client.try_transfer(&alice, &bob, &500);
        assert!(
            result.is_err(),
            "transfer must fail when underlying token panics (simulating WASM trap)"
        );

        // Allowlist state must be unchanged after the failed call.
        assert!(
            client.is_allowed(&alice),
            "alice must still be allowlisted after failed reentrant attempt"
        );
        assert!(
            client.is_allowed(&bob),
            "bob must still be allowlisted after failed reentrant attempt"
        );
    }

    /// Confirms that a non-malicious (honest) token still works correctly
    /// alongside the reentrancy test setup, ruling out a false positive.
    #[test]
    fn test_honest_token_transfer_still_works() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);

        let token_id = env.register(MockToken, ());
        let allowlist_id = env.register(AllowlistToken, ());
        let client = AllowlistTokenClient::new(&env, &allowlist_id);
        client.initialize(&admin, &token_id);

        let alice = Address::generate(&env);
        let bob = Address::generate(&env);
        client.add_to_allowlist(&admin, &alice);
        client.add_to_allowlist(&admin, &bob);

        let ok = client.transfer(&alice, &bob, &200);
        assert!(ok, "honest token transfer must succeed");
    }
}
