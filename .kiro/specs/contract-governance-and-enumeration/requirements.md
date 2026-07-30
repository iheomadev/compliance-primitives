# Requirements Document

## Introduction

This document specifies the requirements for adding secure role transfer and list enumeration capabilities to three existing Soroban smart contracts: `denylist-gate`, `jurisdiction-flag`, and `allowlist-token`. The feature introduces two-step propose-accept patterns for privilege handoff and paginated enumeration for allowlists and denylists, enhancing governance auditability and operational visibility.

## Glossary

- **Admin**: The privileged address authorized to manage the denylist in the `denylist-gate` contract and the allowlist in the `allowlist-token` contract
- **Issuer**: The privileged address authorized to manage jurisdiction codes in the `jurisdiction-flag` contract
- **Pending_Admin**: A proposed Admin address stored temporarily in `DataKey::PendingAdmin` until accepted
- **Pending_Issuer**: A proposed Issuer address stored temporarily in `DataKey::PendingIssuer` until accepted
- **Denylist_Gate**: The Soroban smart contract that maintains a shared on-chain denylist of addresses prohibited from transacting
- **Jurisdiction_Flag**: The Soroban smart contract that attaches jurisdiction codes to addresses for compliance purposes
- **Allowlist_Token**: The Soroban smart contract that wraps a SEP-41 token and restricts transfers to allowlisted addresses
- **List_Index**: A secondary storage structure (Vec<Address>) that enables enumeration of sparse address mappings
- **Page**: A bounded subset of addresses returned by enumeration functions, limited by a maximum size
- **Start_After**: An optional Address parameter used for pagination, indicating the last address from a previous page

## Requirements

### Requirement 1: Admin Transfer for Denylist Gate

**User Story:** As a Denylist_Gate Admin, I want to securely transfer admin privileges to a new address, so that I can rotate keys or hand off governance without risking unauthorized takeover.

#### Acceptance Criteria

1. THE Denylist_Gate SHALL provide a `propose_admin` function that accepts the current Admin address and a new Admin address as parameters
2. WHEN `propose_admin` is called, THE Denylist_Gate SHALL authenticate the current Admin via `require_auth()`
3. WHEN `propose_admin` is called with valid authentication, THE Denylist_Gate SHALL store the new Admin address under `DataKey::PendingAdmin`
4. THE Denylist_Gate SHALL provide an `accept_admin` function that accepts the new Admin address as a parameter
5. WHEN `accept_admin` is called, THE Denylist_Gate SHALL authenticate the caller via `require_auth()`
6. WHEN `accept_admin` is called and the authenticated caller matches the Pending_Admin address, THE Denylist_Gate SHALL replace the stored Admin with the Pending_Admin address
7. WHEN `accept_admin` is called and the authenticated caller matches the Pending_Admin address, THE Denylist_Gate SHALL remove the Pending_Admin from storage
8. WHEN `accept_admin` is called and the authenticated caller matches the Pending_Admin address, THE Denylist_Gate SHALL publish an `AdminTransferred` event containing the old Admin address and the new Admin address
9. WHEN `accept_admin` is called and the authenticated caller does NOT match the Pending_Admin address, THE Denylist_Gate SHALL return `Error::NotAuthorized`
10. WHEN `accept_admin` is called and no Pending_Admin exists in storage, THE Denylist_Gate SHALL return `Error::NotInitialized`

### Requirement 2: Issuer Transfer for Jurisdiction Flag

**User Story:** As a Jurisdiction_Flag Issuer, I want to securely transfer issuer privileges to a new address, so that I can rotate keys or hand off governance without risking unauthorized takeover.

#### Acceptance Criteria

1. THE Jurisdiction_Flag SHALL provide a `propose_issuer` function that accepts the current Issuer address and a new Issuer address as parameters
2. WHEN `propose_issuer` is called, THE Jurisdiction_Flag SHALL authenticate the current Issuer via `require_auth()`
3. WHEN `propose_issuer` is called with valid authentication, THE Jurisdiction_Flag SHALL store the new Issuer address under `DataKey::PendingIssuer`
4. THE Jurisdiction_Flag SHALL provide an `accept_issuer` function that accepts the new Issuer address as a parameter
5. WHEN `accept_issuer` is called, THE Jurisdiction_Flag SHALL authenticate the caller via `require_auth()`
6. WHEN `accept_issuer` is called and the authenticated caller matches the Pending_Issuer address, THE Jurisdiction_Flag SHALL replace the stored Issuer with the Pending_Issuer address
7. WHEN `accept_issuer` is called and the authenticated caller matches the Pending_Issuer address, THE Jurisdiction_Flag SHALL remove the Pending_Issuer from storage
8. WHEN `accept_issuer` is called and the authenticated caller matches the Pending_Issuer address, THE Jurisdiction_Flag SHALL publish an `IssuerTransferred` event containing the old Issuer address and the new Issuer address
9. WHEN `accept_issuer` is called and the authenticated caller does NOT match the Pending_Issuer address, THE Jurisdiction_Flag SHALL return `Error::NotAuthorized`
10. WHEN `accept_issuer` is called and no Pending_Issuer exists in storage, THE Jurisdiction_Flag SHALL return `Error::NotInitialized`

### Requirement 3: Allowlist Enumeration for Allowlist Token

**User Story:** As an auditor or developer, I want to retrieve a paginated list of allowlisted addresses, so that I can verify compliance without reading unbounded contract storage.

#### Acceptance Criteria

1. THE Allowlist_Token SHALL provide a `list_allowed` function that accepts an optional Start_After address and a limit u32 as parameters
2. WHEN `list_allowed` is called, THE Allowlist_Token SHALL return a Vec<Address> containing at most `limit` allowlisted addresses
3. WHEN `list_allowed` is called with Start_After set to Some(address), THE Allowlist_Token SHALL return only addresses that follow the Start_After address in lexicographic order
4. WHEN `list_allowed` is called with Start_After set to None, THE Allowlist_Token SHALL return addresses starting from the first address in lexicographic order
5. WHEN `list_allowed` is called and the number of remaining addresses exceeds `limit`, THE Allowlist_Token SHALL return exactly `limit` addresses
6. WHEN `add_to_allowlist` is called successfully, THE Allowlist_Token SHALL add the address to the List_Index if not already present
7. WHEN `remove_from_allowlist` is called successfully, THE Allowlist_Token SHALL remove the address from the List_Index if present
8. FOR ALL allowlisted addresses, the List_Index SHALL contain that address exactly once
9. FOR ALL addresses in the List_Index, the DataKey::Allowed(Address) storage SHALL return true

### Requirement 4: Denylist Enumeration for Denylist Gate

**User Story:** As an auditor or developer, I want to retrieve a paginated list of denylisted addresses, so that I can verify sanctions compliance without reading unbounded contract storage.

#### Acceptance Criteria

1. THE Denylist_Gate SHALL provide a `list_denied` function that accepts an optional Start_After address and a limit u32 as parameters
2. WHEN `list_denied` is called, THE Denylist_Gate SHALL return a Vec<Address> containing at most `limit` denylisted addresses
3. WHEN `list_denied` is called with Start_After set to Some(address), THE Denylist_Gate SHALL return only addresses that follow the Start_After address in lexicographic order
4. WHEN `list_denied` is called with Start_After set to None, THE Denylist_Gate SHALL return addresses starting from the first address in lexicographic order
5. WHEN `list_denied` is called and the number of remaining addresses exceeds `limit`, THE Denylist_Gate SHALL return exactly `limit` addresses
6. WHEN `add_to_denylist` is called successfully, THE Denylist_Gate SHALL add the address to the List_Index if not already present
7. WHEN `remove_from_denylist` is called successfully, THE Denylist_Gate SHALL remove the address from the List_Index if present
8. FOR ALL denylisted addresses, the List_Index SHALL contain that address exactly once
9. FOR ALL addresses in the List_Index, the DataKey::Denied(Address) storage SHALL return true

### Requirement 5: Shared Pagination Design

**User Story:** As a developer, I want consistent pagination behavior across both enumeration features, so that client code can use a single pagination strategy.

#### Acceptance Criteria

1. THE Allowlist_Token AND THE Denylist_Gate SHALL use identical function signatures for their list enumeration functions
2. THE Allowlist_Token AND THE Denylist_Gate SHALL use identical lexicographic ordering for address sorting
3. THE Allowlist_Token AND THE Denylist_Gate SHALL use identical pagination logic for Start_After handling
4. WHEN a limit of 0 is provided to any enumeration function, THE contract SHALL return an empty Vec<Address>
5. WHEN no addresses exist in a list, THE enumeration function SHALL return an empty Vec<Address>

### Requirement 6: Storage Cost Transparency

**User Story:** As a contract deployer, I want to understand the storage cost implications of list enumeration, so that I can make informed decisions about enabling this feature.

#### Acceptance Criteria

1. THE design documentation SHALL describe the additional storage overhead required for the List_Index
2. THE design documentation SHALL explain that each address in the List_Index incurs persistent storage costs
3. THE design documentation SHALL note that the List_Index duplicates address storage already present in DataKey::Allowed or DataKey::Denied mappings
4. THE design documentation SHALL provide guidance on appropriate limit values for enumeration to avoid excessive per-call costs

### Requirement 7: Authorization for Role Transfer Functions

**User Story:** As a contract security reviewer, I want role transfer functions to enforce strict authorization, so that only the legitimate current role holder can propose a transfer.

#### Acceptance Criteria

1. WHEN `propose_admin` is called by an address that is NOT the current Admin, THE Denylist_Gate SHALL return `Error::NotAuthorized`
2. WHEN `propose_issuer` is called by an address that is NOT the current Issuer, THE Jurisdiction_Flag SHALL return `Error::NotAuthorized`
3. WHEN `propose_admin` or `propose_issuer` is called and the contract has not been initialized, THE contract SHALL return `Error::NotInitialized`

### Requirement 8: Event Auditability for Role Transfers

**User Story:** As a compliance officer, I want role transfer events to be published on-chain, so that I can audit governance changes through event logs.

#### Acceptance Criteria

1. THE AdminTransferred event SHALL include the old Admin address as a topic
2. THE AdminTransferred event SHALL include the new Admin address as a topic
3. THE IssuerTransferred event SHALL include the old Issuer address as a topic
4. THE IssuerTransferred event SHALL include the new Issuer address as a topic
5. WHEN a role transfer is only proposed but not yet accepted, THE contract SHALL NOT publish a transferred event

### Requirement 9: List Index Consistency

**User Story:** As a contract maintainer, I want the List_Index to remain consistent with the sparse address mappings, so that enumeration results are always accurate.

#### Acceptance Criteria

1. WHEN an address is added to a list and already exists in the List_Index, THE contract SHALL NOT duplicate the address in the List_Index
2. WHEN an address is removed from a list and does not exist in the List_Index, THE contract SHALL NOT return an error
3. FOR ALL addresses returned by `list_allowed`, calling `is_allowed` with that address SHALL return true
4. FOR ALL addresses returned by `list_denied`, calling `check` with that address SHALL return false

### Requirement 10: Pagination Edge Cases

**User Story:** As an API consumer, I want enumeration functions to handle edge cases gracefully, so that pagination logic is robust.

#### Acceptance Criteria

1. WHEN `list_allowed` or `list_denied` is called with a limit greater than the number of remaining addresses, THE contract SHALL return all remaining addresses without error
2. WHEN `list_allowed` or `list_denied` is called with Start_After pointing to an address that does NOT exist in the list, THE contract SHALL return addresses following that position in lexicographic order
3. WHEN `list_allowed` or `list_denied` is called with Start_After pointing to the last address in the list, THE contract SHALL return an empty Vec<Address>
4. WHEN `list_allowed` or `list_denied` is called with Start_After pointing to an address lexicographically after all addresses in the list, THE contract SHALL return an empty Vec<Address>
