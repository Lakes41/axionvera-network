# Contract Specification (Vault)

## Initialization

### `initialize(admin, deposit_token, reward_token) -> Result<(), VaultError>`

One-time initialization that sets:
- `admin`: authorized caller for `distribute_rewards`
- `deposit_token`: token used for deposits and withdrawals
- `reward_token`: token used for reward distributions and user claims

Security:
- Fails with `AlreadyInitialized` if called twice.
- Requires `admin` authorization.

Emits:
- `init`

## Deposits

### `deposit(from, amount) -> Result<(), VaultError>`

Transfers `amount` of `deposit_token` from `from` to the contract and increases `from`’s recorded vault balance.

Validations:
- `amount > 0`
- Requires `from` authorization

Accounting:
- Accrues any pending rewards for `from` before changing their balance.

Emits:
- `deposit`

## Withdrawals

### `withdraw(to, amount) -> Result<(), VaultError>`

Transfers `amount` of `deposit_token` from the contract to `to` and decreases `to`’s recorded vault balance.

Validations:
- `amount > 0`
- Requires `to` authorization
- Fails with `InsufficientBalance` if `amount > balance(to)`

Accounting:
- Accrues any pending rewards for `to` before changing their balance.

Emits:
- `withdraw`

## Reward Distribution

### `distribute_rewards(amount) -> Result<i128, VaultError>`

Transfers `amount` of `reward_token` from `admin` to the contract and increases the global `reward_index`.

Validations:
- `amount > 0`
- Requires `admin` authorization
- Fails with `NoDeposits` if `total_deposits == 0`

Emits:
- `distrib`

## Claiming Rewards

### `claim_rewards(user) -> Result<i128, VaultError>`

Accrues pending rewards for `user`, transfers the claimable amount of `reward_token` from the contract to `user`, and resets `user`’s accrued reward counter.

Validations:
- Requires `user` authorization

Emits:
- `claim` (only when amount > 0)

## Views

- `balance(user) -> Result<i128, VaultError>`
- `total_deposits() -> Result<i128, VaultError>`
- `reward_index() -> Result<i128, VaultError>`
- `pending_rewards(user) -> Result<i128, VaultError>`
- `admin() -> Result<Address, VaultError>`
- `deposit_token() -> Result<Address, VaultError>`
- `reward_token() -> Result<Address, VaultError>`

## Errors

See [errors.rs](file:///Users/boufdaddy/Documents/trae_projects/axionvera-network/contracts/vault-contract/src/errors.rs).
