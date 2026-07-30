# ComplianceCheck Trait Design

## Overview

This document describes the design and implementation of a shared `ComplianceCheck` trait that provides a unified interface across all three compliance primitives (`allowlist-token`, `denylist-gate`, `jurisdiction-flag`).

## Problem Statement

Currently, the three compliance contracts have inconsistent calling conventions:

- `allowlist-token.is_allowed(address) -> bool`
- `denylist-gate.check(address) -> bool`
- `jurisdiction-flag.is_permitted_jurisdiction(address, allowed_codes) -> bool`

This inconsistency makes it difficult for external contracts to compose these primitives polymorphically. The `jurisdiction-flag` requires additional parameters, making it impossible to use all three with the same call pattern.

## Solution: ComplianceCheck Trait

### Design Approach

We introduce a shared `ComplianceCheck` trait that standardizes the check interface across all three contracts:

```rust
pub trait ComplianceCheck {
    /// Check if an address passes this compliance check.
    /// Returns true if the address is compliant, false otherwise.
    fn is_compliant(env: Env, address: Address) -> bool;
}
```

### Key Design Decisions

1. **Additive Implementation**: The new trait is added WITHOUT modifying existing contract functions. All current contracts maintain backward compatibility.

2. **Minimum Interface**: The trait uses only the most minimal information (`address -> bool`) to work universally across all three primitives.

3. **Address-Only Semantics**:
   - `allowlist-token`: returns `is_allowed(address)`
   - `denylist-gate`: returns `check(address)` 
   - `jurisdiction-flag`: Requires pre-configuration of permitted codes (stored in contract state) or passed through contract initialization. For direct `is_compliant` calls, the contract can return a default or error. This is the trade-off for unified interface.

### Alternatives Considered

1. **Generic Trait with Associated Types**: Would be more flexible but requires type complexity at call sites.
   - **Rejected**: Soroban contracts don't support generic trait implementations well.

2. **Trait with Multiple Methods**: Could have both `is_compliant` and specialized check methods.
   - **Rejected**: Adds complexity; the goal is a unified interface.

3. **Wrapper Contracts**: Create separate wrapper contracts around each primitive.
   - **Rejected**: Adds deployment overhead and makes debugging harder.

4. **Runtime Polymorphism via Enum**: Pass contract type as enum parameter.
   - **Rejected**: Less idiomatic Rust; doesn't leverage Soroban's contract system.

## Implementation Strategy

### Phase 1: Add Trait Definition

Create a new shared module or keep in existing library code that defines the trait.

### Phase 2: Implement for Each Contract

Add `#[contractimpl]` blocks that implement the trait for each contract:
- `AllowlistToken`
- `DenylistGate`
- `JurisdictionFlag` (with jurisdiction checks disabled or pre-configured)

### Phase 3: Client Generation

Use `#[contractclient]` to generate clients that can call the `is_compliant` function on any of the three contracts.

### Phase 4: Integration Example

Update `rwa-compliance-flow` example to demonstrate:
```rust
// Pseudo-code: calling different contracts through unified interface
let alice = Address::generate(env);

// All three can be called with the same pattern:
let allowlist_ok = allowlist_client.is_compliant(&alice);
let denylist_ok = denylist_client.is_compliant(&alice);
let jurisdiction_ok = jurisdiction_client.is_compliant(&alice);

// All pass -> address is compliant
if allowlist_ok && denylist_ok && jurisdiction_ok {
    // Safe to transfer
}
```

## Backward Compatibility

- ✅ All existing contract functions (`is_allowed`, `check`, `is_permitted_jurisdiction`) remain unchanged
- ✅ All existing tests continue to pass without modification
- ✅ The new `is_compliant` method is purely additive
- ✅ Calling contracts can choose to use either the new unified interface or the existing specialized functions

## Trade-offs

### Simplicity vs. Flexibility

The unified `(address) -> bool` interface sacrifices some flexibility (e.g., `jurisdiction-flag` loses access to permitted codes list) for:
- Easier composition
- Uniform calling convention
- Reduced cognitive load for integrators

### Jurisdiction-Flag Special Case

`jurisdiction-flag` requires additional context (permitted codes) that doesn't fit the minimal interface. Options:

1. **Pre-configuration**: Store permitted codes in contract state; `is_compliant` uses them.
   - **Pro**: Clean interface
   - **Con**: Less flexible for dynamic jurisdiction lists

2. **Default Behavior**: `is_compliant` only checks if ANY jurisdiction is set (not if it's permitted).
   - **Pro**: Works universally
   - **Con**: Doesn't enforce permit list

3. **Return None/Error**: `is_compliant` returns error if codes aren't pre-set.
   - **Pro**: Explicit about limitations
   - **Con**: Breaks uniform interface

**Recommendation**: Option 2 for `jurisdiction-flag` — `is_compliant` returns `true` if the address has ANY jurisdiction set (indicating they've been verified). The check for specific permitted codes remains in `is_permitted_jurisdiction`.

## Future Enhancements

1. **Permit Storage**: Allow contracts to store permitted jurisdiction lists and have `is_compliant` check against them.
2. **Metadata**: Extend trait to include a method returning compliance check type/description.
3. **Composable Checks**: Define a `ComplianceCheckCompositor` trait for AND/OR logic.
4. **Event Standardization**: Standard events emitted on compliance check failures.

## Testing Strategy

- Verify each contract's `is_compliant` behaves consistently with its specialized check
- Test that external contracts can call any of the three through the unified interface
- Ensure all existing tests still pass
- Add new tests specifically for the `is_compliant` method on each contract
