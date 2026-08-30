use crate::{is_paused, pause, require_not_paused, unpause};
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{contract, contractimpl, Address, Env};

/// Minimal harness contract so we can exercise the pausable helpers inside a
/// real Soroban `Env`. All calls go through `env.register(Harness, ())`,
/// which gives each helper a proper contract context with instance storage.
#[contract]
struct Harness;

#[contractimpl]
impl Harness {
    pub fn is_paused(env: Env) -> bool {
        crate::is_paused(&env)
    }

    pub fn pause(env: Env) {
        crate::pause(&env);
    }

    pub fn unpause(env: Env) {
        crate::unpause(&env);
    }

    /// Calls `require_not_paused` and returns `true` if the call succeeds
    /// (i.e. the contract is not paused). Used to verify the happy path
    /// without a separate entry point.
    pub fn check_not_paused(env: Env) -> bool {
        crate::require_not_paused(&env);
        true
    }
}

// ─── is_paused default ──────────────────────────────────────────────────────

#[test]
fn test_is_paused_default_is_false() {
    let env = Env::default();
    let id = env.register(Harness, ());
    let client = HarnessClient::new(&env, &id);
    assert!(!client.is_paused());
}

// ─── pause ──────────────────────────────────────────────────────────────────

#[test]
fn test_pause_sets_flag() {
    let env = Env::default();
    let id = env.register(Harness, ());
    let client = HarnessClient::new(&env, &id);

    assert!(!client.is_paused());
    client.pause();
    assert!(client.is_paused());
}

#[test]
fn test_pause_is_idempotent() {
    let env = Env::default();
    let id = env.register(Harness, ());
    let client = HarnessClient::new(&env, &id);

    client.pause();
    client.pause(); // second call must not panic or corrupt state
    assert!(client.is_paused());
}

// ─── unpause ────────────────────────────────────────────────────────────────

#[test]
fn test_unpause_clears_flag() {
    let env = Env::default();
    let id = env.register(Harness, ());
    let client = HarnessClient::new(&env, &id);

    client.pause();
    assert!(client.is_paused());

    client.unpause();
    assert!(!client.is_paused());
}

#[test]
fn test_unpause_when_not_paused_is_noop() {
    let env = Env::default();
    let id = env.register(Harness, ());
    let client = HarnessClient::new(&env, &id);

    // Never paused — unpause must not panic
    client.unpause();
    assert!(!client.is_paused());
}

#[test]
fn test_pause_unpause_roundtrip() {
    let env = Env::default();
    let id = env.register(Harness, ());
    let client = HarnessClient::new(&env, &id);

    client.pause();
    assert!(client.is_paused());
    client.unpause();
    assert!(!client.is_paused());
    client.pause();
    assert!(client.is_paused());
}

// ─── require_not_paused ─────────────────────────────────────────────────────

#[test]
fn test_require_not_paused_succeeds_when_active() {
    let env = Env::default();
    let id = env.register(Harness, ());
    let client = HarnessClient::new(&env, &id);

    // Not paused — must not panic
    assert!(client.check_not_paused());
}

#[test]
#[should_panic]
fn test_require_not_paused_panics_when_paused() {
    let env = Env::default();
    let id = env.register(Harness, ());
    let client = HarnessClient::new(&env, &id);

    client.pause();
    // This call must panic because the contract is paused
    client.check_not_paused();
}
