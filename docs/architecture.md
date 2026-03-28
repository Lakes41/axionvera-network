# Architecture

## Contract Overview

The Axionvera Vault contract is an index-based vault with lazy reward accrual:
- Users deposit a configured `deposit_token` into the contract.
- The contract tracks per-user balances and a `total_deposits` aggregate.
- Rewards are distributed proportionally to current deposit balances by increasing a global `reward_index`.
- Users claim realized rewards via `claim_rewards`, and their rewards are accounted for lazily on every interaction.

Key files:
- [lib.rs](file:///Users/boufdaddy/Documents/trae_projects/axionvera-network/contracts/vault-contract/src/lib.rs) - public contract interface
- [storage.rs](file:///Users/boufdaddy/Documents/trae_projects/axionvera-network/contracts/vault-contract/src/storage.rs) - storage schema and reward math helpers
- [events.rs](file:///Users/boufdaddy/Documents/trae_projects/axionvera-network/contracts/vault-contract/src/events.rs) - event types and emitters
- [errors.rs](file:///Users/boufdaddy/Documents/trae_projects/axionvera-network/contracts/vault-contract/src/errors.rs) - contract error codes

## Storage Design

### Instance Storage (Global)

Stored under `Env.storage().instance()`:
- `State`: packed global struct containing:
- `admin`: contract administrator address
- `deposit_token`: token contract address used for deposits and withdrawals
- `reward_token`: token contract address used for reward distributions and claims
- `total_deposits`: total deposited amount across all users
- `reward_index`: cumulative reward-per-share index

### Persistent Storage (Per User)

Stored under `Env.storage().persistent()` keyed by user address:
- `User(Address)`: packed per-user position containing:
- `balance`: deposited amount
- `reward_index`: user snapshot of `RewardIndex`
- `rewards`: accrued but unclaimed rewards

Storage entries are TTL-bumped on access to keep active accounts alive.

## Event System

All state-changing actions emit an event:
- `init` - initialization parameters
- `deposit` - depositor, amount, and resulting balance
- `withdraw` - withdrawer, amount, and resulting balance
- `distrib` - caller, amount distributed, resulting `reward_index`
- `claim` - claimer and amount claimed

Event payloads are defined in [events.rs](file:///Users/boufdaddy/Documents/trae_projects/axionvera-network/contracts/vault-contract/src/events.rs).

## Reward Accounting

`reward_index` is scaled by `1e18` to preserve precision.

When `distribute_rewards(amount)` is called:
- `reward_index += (amount * 1e18) / total_deposits`

When a user interacts (deposit, withdraw, claim):
- Compute `delta = reward_index - user.reward_index`
- Accrue `balance * delta / 1e18` into `user.rewards`
- Set `user.reward_index = reward_index`

This avoids iterating over depositors and keeps distribution `O(1)`.

## Extension Points (Good First Issues)

- Reward rounding strategy and dust handling
- Gas and storage read optimizations
- Additional security checks (pause, caps, allowlists)
- Governance patterns (admin handover, multisig)
- Upgrade patterns compatible with Soroban best practices
