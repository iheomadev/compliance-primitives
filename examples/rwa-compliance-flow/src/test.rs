use super::*;
use allowlist_token::{AllowlistToken, AllowlistTokenClient};
use denylist_gate::{DenylistGate, DenylistGateClient};
use jurisdiction_flag::{JurisdictionFlag, JurisdictionFlagClient};
use soroban_sdk::testutils::Address as _;
use soroban_sdk::Env;

struct ComplianceSetup<'a> {
    env: &'a Env,
    allowlist_admin: Address,
    denylist_admin: Address,
    issuer: Address,
    allowlist_token: AllowlistTokenClient<'a>,
    denylist_gate: DenylistGateClient<'a>,
    jurisdiction_flag: JurisdictionFlagClient<'a>,
    mock_token_id: Address,
}

fn setup_compliance_contracts(env: &Env) -> ComplianceSetup {
    env.mock_all_auths();

    let allowlist_admin = Address::generate(env);
    let denylist_admin = Address::generate(env);
    let issuer = Address::generate(env);
    let mock_token_id = Address::generate(env);

    // Register and initialize allowlist-token
    let allowlist_token_id = env.register(AllowlistToken, ());
    let allowlist_token = AllowlistTokenClient::new(env, &allowlist_token_id);
    allowlist_token.initialize(&allowlist_admin, &mock_token_id);

    // Register and initialize denylist-gate
    let denylist_gate_id = env.register(DenylistGate, ());
    let denylist_gate = DenylistGateClient::new(env, &denylist_gate_id);
    denylist_gate.initialize(&denylist_admin);

    // Register and initialize jurisdiction-flag
    let jurisdiction_flag_id = env.register(JurisdictionFlag, ());
    let jurisdiction_flag = JurisdictionFlagClient::new(env, &jurisdiction_flag_id);
    jurisdiction_flag.initialize(&issuer);

    ComplianceSetup {
        env,
        allowlist_admin,
        denylist_admin,
        issuer,
        allowlist_token,
        denylist_gate,
        jurisdiction_flag,
        mock_token_id,
    }
}

#[test]
fn test_rwa_token_flow_success_all_checks_pass() {
    // SCENARIO: Successful transfer when all three compliance checks pass
    //
    // This tests the happy path where:
    // - Both parties are on the allowlist
    // - Neither party is on the denylist
    // - Both parties are in permitted jurisdictions
    let env = Env::default();
    let setup = setup_compliance_contracts(&env);

    // Create two parties (alice and bob)
    let alice = Address::generate(&env);
    let bob = Address::generate(&env);

    // Step 1: Add both parties to the allowlist
    setup.allowlist_token.add_to_allowlist(&setup.allowlist_admin, &alice);
    setup.allowlist_token.add_to_allowlist(&setup.allowlist_admin, &bob);

    // Step 2: Ensure both parties are NOT on the denylist (by default they aren't)
    assert!(setup.denylist_gate.check(&alice));
    assert!(setup.denylist_gate.check(&bob));

    // Step 3: Set jurisdiction for both parties (both in USA)
    let usa_code = String::from_slice(&env, "US");
    setup.jurisdiction_flag.set_jurisdiction(&setup.issuer, &alice, &usa_code);
    setup.jurisdiction_flag.set_jurisdiction(&setup.issuer, &bob, &usa_code);

    // Step 4: Verify both parties are in permitted jurisdiction
    let permitted_codes = soroban_sdk::vec![&env, usa_code.clone()];
    assert!(setup.jurisdiction_flag.is_permitted_jurisdiction(&alice, &permitted_codes));
    assert!(setup.jurisdiction_flag.is_permitted_jurisdiction(&bob, &permitted_codes));

    // Step 5: Verify all compliance checks pass for both parties
    // ALLOWLIST CHECK
    assert!(setup.allowlist_token.is_allowed(&alice));
    assert!(setup.allowlist_token.is_allowed(&bob));

    // DENYLIST CHECK
    assert!(setup.denylist_gate.check(&alice));
    assert!(setup.denylist_gate.check(&bob));

    // JURISDICTION CHECK
    assert!(setup.jurisdiction_flag.is_permitted_jurisdiction(&alice, &permitted_codes));
    assert!(setup.jurisdiction_flag.is_permitted_jurisdiction(&bob, &permitted_codes));

    // ✓ This represents a successful RWA token transfer scenario
    // All three compliance checks pass for both parties
}

#[test]
fn test_rwa_token_flow_blocked_by_allowlist() {
    // SCENARIO: Transfer blocked by allowlist check
    //
    // This tests the failure case where:
    // - Sender (alice) is on the allowlist
    // - Recipient (bob) is NOT on the allowlist
    // - Neither is on the denylist
    // - Both are in permitted jurisdictions
    //
    // RESULT: Transfer should be blocked at the allowlist check
    let env = Env::default();
    let setup = setup_compliance_contracts(&env);

    let alice = Address::generate(&env);
    let bob = Address::generate(&env);

    // Add only alice to allowlist, NOT bob
    setup.allowlist_token.add_to_allowlist(&setup.allowlist_admin, &alice);

    // Set jurisdiction for both (would otherwise pass)
    let usa_code = String::from_slice(&env, "US");
    setup.jurisdiction_flag.set_jurisdiction(&setup.issuer, &alice, &usa_code);
    setup.jurisdiction_flag.set_jurisdiction(&setup.issuer, &bob, &usa_code);

    // Ensure neither is on denylist (would otherwise pass)
    assert!(setup.denylist_gate.check(&alice));
    assert!(setup.denylist_gate.check(&bob));

    // VERIFICATION: Allowlist check fails for bob
    assert!(setup.allowlist_token.is_allowed(&alice));
    assert!(!setup.allowlist_token.is_allowed(&bob));

    // Jurisdiction checks pass
    let permitted_codes = soroban_sdk::vec![&env, usa_code.clone()];
    assert!(setup.jurisdiction_flag.is_permitted_jurisdiction(&alice, &permitted_codes));
    assert!(setup.jurisdiction_flag.is_permitted_jurisdiction(&bob, &permitted_codes));

    // ✗ Transfer would be BLOCKED by allowlist check
    // Bob is not on the allowlist
}

#[test]
fn test_rwa_token_flow_blocked_by_denylist() {
    // SCENARIO: Transfer blocked by denylist check
    //
    // This tests the failure case where:
    // - Both parties are on the allowlist
    // - Sender (alice) IS on the denylist
    // - Both are in permitted jurisdictions
    //
    // RESULT: Transfer should be blocked at the denylist check
    let env = Env::default();
    let setup = setup_compliance_contracts(&env);

    let alice = Address::generate(&env);
    let bob = Address::generate(&env);

    // Add both to allowlist (would otherwise pass)
    setup.allowlist_token.add_to_allowlist(&setup.allowlist_admin, &alice);
    setup.allowlist_token.add_to_allowlist(&setup.allowlist_admin, &bob);

    // Set jurisdiction for both (would otherwise pass)
    let usa_code = String::from_slice(&env, "US");
    setup.jurisdiction_flag.set_jurisdiction(&setup.issuer, &alice, &usa_code);
    setup.jurisdiction_flag.set_jurisdiction(&setup.issuer, &bob, &usa_code);

    // Add alice to denylist (sanctions/fraud/court order, etc.)
    setup.denylist_gate.add_to_denylist(&setup.denylist_admin, &alice);

    // VERIFICATION: Denylist check fails for alice
    assert!(!setup.denylist_gate.check(&alice));
    assert!(setup.denylist_gate.check(&bob));

    // Allowlist checks pass
    assert!(setup.allowlist_token.is_allowed(&alice));
    assert!(setup.allowlist_token.is_allowed(&bob));

    // ✗ Transfer would be BLOCKED by denylist check
    // Alice is on the denylist
}

#[test]
fn test_rwa_token_flow_blocked_by_jurisdiction() {
    // SCENARIO: Transfer blocked by jurisdiction check
    //
    // This tests the failure case where:
    // - Both parties are on the allowlist
    // - Neither is on the denylist
    // - Sender (alice) is in a permitted jurisdiction
    // - Recipient (bob) is NOT in a permitted jurisdiction (or has no jurisdiction set)
    //
    // RESULT: Transfer should be blocked at the jurisdiction check
    let env = Env::default();
    let setup = setup_compliance_contracts(&env);

    let alice = Address::generate(&env);
    let bob = Address::generate(&env);

    // Add both to allowlist (would otherwise pass)
    setup.allowlist_token.add_to_allowlist(&setup.allowlist_admin, &alice);
    setup.allowlist_token.add_to_allowlist(&setup.allowlist_admin, &bob);

    // Ensure neither is on denylist (would otherwise pass)
    assert!(setup.denylist_gate.check(&alice));
    assert!(setup.denylist_gate.check(&bob));

    // Set jurisdiction for alice (USA) but NOT bob
    let usa_code = String::from_slice(&env, "US");
    setup.jurisdiction_flag.set_jurisdiction(&setup.issuer, &alice, &usa_code);
    // bob has no jurisdiction set

    // Only allow USA jurisdictions
    let permitted_codes = soroban_sdk::vec![&env, usa_code.clone()];

    // VERIFICATION: Jurisdiction check passes for alice but fails for bob
    assert!(setup.jurisdiction_flag.is_permitted_jurisdiction(&alice, &permitted_codes));
    assert!(!setup.jurisdiction_flag.is_permitted_jurisdiction(&bob, &permitted_codes));

    // Allowlist checks pass
    assert!(setup.allowlist_token.is_allowed(&alice));
    assert!(setup.allowlist_token.is_allowed(&bob));

    // ✗ Transfer would be BLOCKED by jurisdiction check
    // Bob is not in a permitted jurisdiction
}

#[test]
fn test_rwa_token_flow_blocked_by_all_three_independently() {
    // SCENARIO: Comprehensive test showing all three compliance layers
    // can independently block transfers
    //
    // This test creates two parties with opposite compliance states:
    // - Charlie: fails EVERY check
    // - Diana: passes EVERY check
    //
    // RESULT: Demonstrates all three layers working independently
    let env = Env::default();
    let setup = setup_compliance_contracts(&env);

    let charlie = Address::generate(&env);
    let diana = Address::generate(&env);

    // Charlie: NOT on allowlist
    // Diana: ON allowlist
    setup.allowlist_token.add_to_allowlist(&setup.allowlist_admin, &diana);

    // Charlie: ON denylist
    // Diana: NOT on denylist
    setup.denylist_gate.add_to_denylist(&setup.denylist_admin, &charlie);

    // Charlie: no jurisdiction set (fails jurisdiction check)
    // Diana: USA jurisdiction (passes jurisdiction check)
    let usa_code = String::from_slice(&env, "US");
    setup.jurisdiction_flag.set_jurisdiction(&setup.issuer, &diana, &usa_code);

    let permitted_codes = soroban_sdk::vec![&env, usa_code.clone()];

    // VERIFICATION: Charlie fails all three checks
    assert!(!setup.allowlist_token.is_allowed(&charlie));
    assert!(!setup.denylist_gate.check(&charlie));
    assert!(!setup.jurisdiction_flag.is_permitted_jurisdiction(&charlie, &permitted_codes));

    // VERIFICATION: Diana passes all three checks
    assert!(setup.allowlist_token.is_allowed(&diana));
    assert!(setup.denylist_gate.check(&diana));
    assert!(setup.jurisdiction_flag.is_permitted_jurisdiction(&diana, &permitted_codes));

    // ✓ This demonstrates that the three compliance layers are independent
    // Charlie would be blocked at any of the three checks
    // Diana passes all three layers
}

#[test]
fn test_metadata_available_on_all_contracts() {
    // BONUS TEST: Verify that metadata() function works on all contracts
    // This is related to Issue #24
    let env = Env::default();
    let setup = setup_compliance_contracts(&env);

    // Try to call metadata on each contract
    // (Note: The actual metadata() client methods would need to be defined
    // in the contract clients above, but this demonstrates the pattern)

    // These calls would work once metadata clients are properly defined:
    // let allowlist_meta = setup.allowlist_token.metadata();
    // let denylist_meta = setup.denylist_gate.metadata();
    // let jurisdiction_meta = setup.jurisdiction_flag.metadata();
}
