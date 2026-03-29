# Vault Contract Specification

This document explains the Soroban vault contract in practical terms for contributors who are new to the codebase.

The vault supports four core user flows:

1. A user deposits the `deposit_token`.
2. The contract tracks that user's vault balance.
3. An admin distributes rewards using the `reward_token`.
4. Users later claim their accrued rewards.

If you want the storage-level view first, read [contract-storage.md](/c:/Users/ADMIN/Desktop/remmy-drips/axionvera-network/docs/contract-storage.md).

## Contract Purpose

The contract acts like a token vault with lazy reward accounting.

- Deposits are tracked 1:1 in token units.
- Rewards are not pushed to every user immediately.
- Instead, the contract updates a global `reward_index`.
- Each user realizes their share when they interact again through `deposit`, `withdraw`, or `claim_rewards`.

This approach keeps reward distribution efficient because the contract does not need to iterate through every depositor.

## Storage Model Summary

The contract stores:

- global configuration: admin address, deposit token, reward token, initialization flag
- global accounting: `total_deposits`, `reward_index`
- per-user accounting: `user_balance`, `user_reward_index`, `user_rewards`

See [contract-storage.md](/c:/Users/ADMIN/Desktop/remmy-drips/axionvera-network/docs/contract-storage.md) for the full storage breakdown.

## Reward Accounting Model

Rewards use an index scaled by `1e18`:

`reward_index += amount * 1e18 / total_deposits`

When a user interacts, the contract compares:

- the global `reward_index`
- that user's saved `user_reward_index`

The difference tells the contract how much new reward has accrued since the user's last interaction.

## Public Functions

### `version() -> u32`

Returns the contract version.

Why it exists:
- useful for integrations, upgrades, and quick sanity checks after deployment

Example:

```rust
let version = vault.version();
assert_eq!(version, 1);
```

### `initialize(admin, deposit_token, reward_token) -> Result<(), VaultError>`

Performs one-time setup for the contract.

What it does:
- stores the admin address
- stores the deposit token address
- stores the reward token address
- resets `total_deposits` and `reward_index` to `0`
- emits an `init` event

Security:
- Fails with `AlreadyInitialized` if called twice.
- Fails with `InvalidTokenConfiguration` if `deposit_token == reward_token`.
- Requires `admin` authorization.
Important rules:
- can only run once
- requires `admin` authorization

Example:

```rust
vault.initialize(&admin, &deposit_token_id, &reward_token_id);
```

### `deposit(from, amount) -> Result<(), VaultError>`

Moves deposit tokens from the user into the vault and increases their recorded vault balance.

Validations:
- `amount > 0`
- Requires `from` authorization
- Fails with `InsufficientBalance` if `from` does not hold enough `deposit_token`

Accounting:
- Accrues any pending rewards for `from` before changing their balance.
- Rejects invalid transfers before mutating user reward snapshots or vault balances.
Step-by-step:

1. Confirms the contract is initialized.
2. Validates `amount > 0`.
3. Requires authorization from `from`.
4. Accrues any rewards already owed to `from`.
5. Transfers `deposit_token` from the user into the contract.
6. Increases `user_balance(from)`.
7. Increases `total_deposits`.
8. Emits a `deposit` event.

Why reward accrual happens first:
- the user should receive rewards based on their old balance up to this point in time
- only after that should the new deposit affect future distributions

Example:

```rust
vault.deposit(&user, &400);
assert_eq!(vault.balance(&user), 400);
assert_eq!(vault.total_deposits(), 400);
```

### `withdraw(to, amount) -> Result<(), VaultError>`

Moves deposit tokens from the vault back to the user and reduces their recorded vault balance.

Step-by-step:

Validations:
- `amount > 0`
- Requires `to` authorization
- Fails with `InsufficientBalance` if `amount > balance(to)`
- Fails with `InsufficientContractBalance` if the vault cannot cover the token transfer

Accounting:
- Accrues any pending rewards for `to` before changing their balance.
- Final state is only written after token transfer pre-checks succeed.
1. Confirms the contract is initialized.
2. Validates `amount > 0`.
3. Requires authorization from `to`.
4. Accrues any rewards already owed to `to`.
5. Checks the user has enough deposited balance.
6. Decreases `user_balance(to)`.
7. Decreases `total_deposits`.
8. Transfers `deposit_token` back to the user.
9. Emits a `withdraw` event.

Fails when:
- the amount is zero or negative
- the user tries to withdraw more than their balance

Example:

```rust
vault.deposit(&user, &400);
vault.withdraw(&user, &150);

assert_eq!(vault.balance(&user), 250);
assert_eq!(vault.total_deposits(), 250);
```

### `distribute_rewards(amount) -> Result<i128, VaultError>`

Transfers reward tokens from the admin into the contract and updates the global reward index.

Step-by-step:

Validations:
- `amount > 0`
- Requires `admin` authorization
- Fails with `NoDeposits` if `total_deposits == 0`
- Fails with `InsufficientBalance` if `admin` does not hold enough `reward_token`
1. Confirms the contract is initialized.
2. Validates `amount > 0`.
3. Requires admin authorization.
4. Verifies `total_deposits > 0`.
5. Transfers `reward_token` from the admin into the contract.
6. Computes the reward-index increment.
7. Updates the global `reward_index`.
8. Emits a `distrib` event.
9. Returns the new `reward_index`.

Important behavior:
- this does not immediately transfer rewards to users
- it only updates global accounting so users can realize rewards later

Example:

```rust
let next_index = vault.distribute_rewards(&400);
assert!(next_index > 0);
```

### `claim_rewards(user) -> Result<i128, VaultError>`

Pays the user the rewards that have already accrued for them.

Step-by-step:

1. Confirms the contract is initialized.
2. Requires authorization from `user`.
3. Accrues any newly earned rewards into `user_rewards`.
4. Reads the current claimable amount.
5. Returns `0` immediately if nothing is claimable.
6. Resets `user_rewards(user)` to `0`.
7. Transfers `reward_token` from the contract to the user.
8. Emits a `claim` event when a transfer happens.

Example:

```rust
let claimed = vault.claim_rewards(&user);
assert!(claimed >= 0);
```

### `balance(user) -> Result<i128, VaultError>`

Returns the user's deposited vault balance.

### `total_deposits() -> Result<i128, VaultError>`

Returns the total amount of deposit tokens currently represented inside the vault.

### `reward_index() -> Result<i128, VaultError>`

Returns the current global reward index.

### `pending_rewards(user) -> Result<i128, VaultError>`

Returns the user's claimable rewards without mutating storage.

Example:

```rust
let pending = vault.pending_rewards(&user);
```

### `admin() -> Result<Address, VaultError>`

Returns the configured admin address.

### `deposit_token() -> Result<Address, VaultError>`

Returns the deposit token contract address.

### `reward_token() -> Result<Address, VaultError>`

Returns the reward token contract address.

## Events

The contract emits structured events for important state changes.

### `init`

Fields:
- `admin`
- `deposit_token`
- `reward_token`
- `timestamp`

### `deposit`

Fields:
- `user`
- `amount`
- `new_balance`
- `timestamp`

### `withdraw`

Fields:
- `user`
- `amount`
- `new_balance`
- `timestamp`

### `distrib`

Fields:
- `caller`
- `amount`
- `reward_index`
- `timestamp`

### `claim`

Fields:
- `user`
- `amount`
- `timestamp`

## Errors

- `AlreadyInitialized`: vault initialization can only happen once.
- `NotInitialized`: the vault must be initialized before use.
- `InvalidAmount`: token amounts must be greater than zero.
- `InsufficientBalance`: the caller-facing token balance is lower than the requested amount.
- `NoDeposits`: rewards cannot be distributed while `total_deposits == 0`.
- `InvalidTokenConfiguration`: deposit and reward token addresses must be different.
- `InsufficientContractBalance`: the vault does not hold enough tokens to complete the transfer.
- `MathOverflow`: arithmetic overflow or underflow was detected while updating accounting.
The contract can return the following errors from [errors.rs](/c:/Users/ADMIN/Desktop/remmy-drips/axionvera-network/contracts/vault-contract/src/errors.rs):

- `AlreadyInitialized`
- `NotInitialized`
- `Unauthorized`
- `InvalidAmount`
- `InsufficientBalance`
- `MathOverflow`
- `NoDeposits`

## Typical End-to-End Flow

1. Deploy the contract.
2. Call `initialize`.
3. User A deposits.
4. User B deposits.
5. Admin calls `distribute_rewards`.
6. Users inspect `pending_rewards`.
7. Users call `claim_rewards`.

## Contributor Tips

- Read [contracts/vault-contract/src/lib.rs](/c:/Users/ADMIN/Desktop/remmy-drips/axionvera-network/contracts/vault-contract/src/lib.rs) for the public API.
- Read [contracts/vault-contract/src/storage.rs](/c:/Users/ADMIN/Desktop/remmy-drips/axionvera-network/contracts/vault-contract/src/storage.rs) for accounting internals.
- Start with the tests in [contracts/vault-contract/src/lib.rs](/c:/Users/ADMIN/Desktop/remmy-drips/axionvera-network/contracts/vault-contract/src/lib.rs) if you want executable examples.
