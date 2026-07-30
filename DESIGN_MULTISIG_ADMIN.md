# Multi-Admin M-of-N Multisig Design

## Overview

This document describes the design and implementation of M-of-N multisignature (multisig) support for administrative operations in the compliance primitives, specifically focusing on the `denylist-gate` contract.

## Problem Statement

Current single-admin design creates operational risk:
- Single point of failure: if the admin key is compromised, the entire denylist can be tampered with
- No distributed decision-making: RWA issuers often need governance approval for compliance changes
- No audit trail of decisions: can't trace who approved what

Multi-signature support allows N signers to require M approvals before executing sensitive operations.

## Solution: M-of-N Multisig for Denylist-Gate

### Design Approach

We implement M-of-N multisignature validation for administrative operations in `denylist-gate`:

```rust
pub struct SignerSet {
    pub signers: Vec<Address>,     // Set of N authorized signers
    pub threshold: u32,            // M: required number of signatures
}
```

### Key Design Decisions

1. **Target Contract**: `denylist-gate` is chosen for implementation because:
   - Simpler state model (only maintains a denylist, no token transfers)
   - Clear use case: sanctions list administration requires careful control
   - Easier to test and reason about

2. **Backward Compatibility**: Support for both single-admin and multi-admin modes:
   - Contracts can initialize with a single admin (existing behavior)
   - OR with a signer set and threshold (new behavior)
   - Migration path: single admin can promote themselves + others into signer set

3. **Authorization Model**:
   - Each signer independently calls `add_to_denylist`, `remove_from_denylist`
   - Signatures are counted and verified using Soroban's auth context
   - Operation executes when threshold is met
   - Uses Soroban's native `require_auth()` primitives

4. **Signer Set Management**:
   - Only existing signers (requiring consensus) can modify the signer set
   - Requires M-of-current-signers approval to add/remove signers
   - Prevents rogue signer from unilaterally modifying the set

### Storage Architecture

```rust
#[contracttype]
enum DataKey {
    // Backward compatible single-admin mode
    Admin,                              // Stores single Address
    
    // Multi-admin mode
    SignerSet,                          // Stores Vec<Address> of signers
    Threshold,                          // Stores u32 for M-of-N
    
    // Denylist entries (unchanged)
    Denied(Address),
}
```

### Threshold Validation Logic

```
When admin/signer calls add_to_denylist or remove_from_denylist:
  1. Check if contract is in multi-admin mode (has SignerSet)
  2. If single-admin mode:
     - Verify caller is the stored Admin
     - Execute immediately
  3. If multi-admin mode:
     - Verify caller is in the signer set
     - Count valid signatures via Soroban auth context
     - If signature_count >= threshold:
        - Execute operation
        - Emit event with operation hash and signers
     - Else:
        - Return error (threshold not met)
```

### Operation Flow: Adding to Denylist with M-of-N

**Scenario**: 3-of-5 multisig (3 signatures required out of 5 signers)

```
1. Signer A calls: add_to_denylist(env, signers=[A,B,C], address, threshold=3)
   - Validates A is in signer set
   - Counts signatures: 1 (only A has called)
   - Result: Pending (1/3)

2. Signer B calls: add_to_denylist(env, signers=[A,B,C], address, threshold=3)
   - Validates B is in signer set
   - Counts signatures: 2 (A and B have now signed)
   - Result: Pending (2/3)

3. Signer C calls: add_to_denylist(env, signers=[A,B,C], address, threshold=3)
   - Validates C is in signer set
   - Counts signatures: 3 (A, B, and C have signed)
   - Result: SUCCESS - Address added to denylist
   - Emit event: DenyAdd { address, signers_called: [A, B, C] }
```

### Signer Set Modification

To add a new signer (requires M-of-N current signers):

```
1. Existing signers collectively call: add_signer(env, new_signer_address)
   - Requires M-of-current-signers approval
   - On threshold met: new_signer joins the set
   - Emit event: SignerAdded { new_signer, approved_by: [signer_addresses] }

2. Similar flow for: remove_signer(env, signer_to_remove_address)
   - Cannot remove down to 0 signers
   - Cannot remove if it would make threshold impossible
```

### Error Handling

```rust
pub enum Error {
    NotInitialized = 1,
    AlreadyInitialized = 2,
    NotAuthorized = 3,
    ThresholdNotMet = 4,           // NEW: not enough signatures
    InvalidThreshold = 5,           // NEW: threshold > signer count
    InvalidSignerSet = 6,           // NEW: empty signer set
    SignerNotInSet = 7,            // NEW: caller not in signer set
}
```

## Implementation Phases

### Phase 1: Core Multisig Logic
- Add multisig storage keys to DataKey enum
- Implement threshold counting via Soroban auth
- Add validation functions

### Phase 2: Dual-Mode Support
- Modify `add_to_denylist` and `remove_from_denylist` to support both modes
- Preserve single-admin path for backward compatibility
- Add new operations: `initialize_multisig`, `add_signer`, `remove_signer`

### Phase 3: Testing
- Unit tests for threshold validation
- Tests for signer set modifications
- Backward compatibility tests
- Edge cases (empty signers, invalid threshold, etc.)

## Trade-offs

### Flexibility vs. Simplicity
- Approach: Require all N signers to participate (signature counting)
- Pro: Clear, auditable, uses Soroban native auth
- Con: All signers must sign; can't use pre-signed transactions

### Storage Efficiency
- Store full signer list in every call
- Pro: Verifiable, auditable, clear who is authorized
- Con: Higher transaction costs
- Future: Could compress with signer set hash

### Governance Model
- Each signer independently calls the function
- Pro: Distributed, no coordinator needed
- Con: Requires off-chain coordination
- Future: Could add coordinator pattern

## Backward Compatibility

1. **Existing Contracts**: Continue using single-admin mode
   - No changes required
   - `Admin` key remains the source of truth
   - `initialize(env, admin)` works as before

2. **Migration Path**: Single admin can adopt multisig
   - Call new `initialize_multisig(env, signers=[...], threshold=M)`
   - Requires existing admin to approve
   - Sets new mode but preserves admin address as initial signer

3. **Mixed Mode** (Not Recommended):
   - Contracts should choose single-admin OR multisig
   - Not supporting both simultaneously prevents confusion

## Testing Strategy

### Unit Tests
- `test_single_admin_still_works()` - backward compatibility
- `test_multisig_execute_at_threshold()` - M of N signers
- `test_multisig_reject_below_threshold()` - < M signers
- `test_invalid_threshold()` - threshold > N
- `test_signer_not_in_set()` - unauthorized signer
- `test_add_signer()` - modify signer set
- `test_remove_signer()` - modify signer set
- `test_cannot_remove_last_signer()` - safety check
- `test_threshold_makes_impossible()` - threshold > new count after removal

### Integration Tests
- `test_3_of_5_multisig_full_flow()` - realistic scenario
- `test_multisig_with_concurrent_calls()` - order independence

## Security Considerations

1. **Replay Protection**: Each `add_to_denylist` call is independent
   - Soroban's auth system prevents replay attacks
   - Different operation = different authorization needed

2. **Signer Modification Safety**:
   - Requires M-of-N approval (high bar)
   - Cannot remove last signer
   - Cannot remove too many to make threshold impossible

3. **Griefing Protection**:
   - If signer list changes mid-operation, operation fails
   - Clients should detect this and restart with updated signer set

## Future Enhancements

1. **Time-based Delays**: Add timelock after threshold met
2. **Coordinator Pattern**: Designate one signer as transaction coordinator
3. **Signature Caching**: Store signatures for later replay within time window
4. **Weighted Voting**: Different signers have different weights
5. **Signer Rotation**: Auto-rotate signers on schedule
