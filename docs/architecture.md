# Architecture

## Contract Overview

The Axionvera Vault contract is an index-based vault with lazy reward accrual:
- Users deposit a configured `deposit_token` into the contract.
- The contract tracks per-user balances and a `total_deposits` aggregate.
- Rewards are distributed proportionally to current deposit balances by increasing a global `reward_index`.
- Users claim realized rewards via `claim_rewards`, and their rewards are accounted for lazily on every interaction.

Key files:
- [lib.rs](file:///Users/boufdaddy/Documents/trae_projects/axionvera-network/contracts/vault-contract/src/lib.rs) — public contract interface
- [storage.rs](file:///Users/boufdaddy/Documents/trae_projects/axionvera-network/contracts/vault-contract/src/storage.rs) — storage schema + reward math helpers
- [events.rs](file:///Users/boufdaddy/Documents/trae_projects/axionvera-network/contracts/vault-contract/src/events.rs) — event types and emitters
- [errors.rs](file:///Users/boufdaddy/Documents/trae_projects/axionvera-network/contracts/vault-contract/src/errors.rs) — contract error codes

## Storage Design

### Instance Storage (Global)

Stored under `Env.storage().instance()`:
- `Admin`: contract administrator address
- `DepositToken`: token contract address used for deposits/withdrawals
- `RewardToken`: token contract address used for reward distributions/claims
- `TotalDeposits`: total deposited amount (sum of user balances)
- `RewardIndex`: cumulative reward-per-share index

### Persistent Storage (Per User)

Stored under `Env.storage().persistent()` keyed by user address:
- `UserBalance(Address)`: deposited amount
- `UserRewardIndex(Address)`: user’s last observed `RewardIndex` snapshot
- `UserRewards(Address)`: accrued but unclaimed rewards

Storage entries are TTL-bumped on access to keep active accounts alive.

## Event System

All state-changing actions emit an event:
- `init` — initialization parameters
- `deposit` — depositor, amount, and resulting balance
- `withdraw` — withdrawer, amount, and resulting balance
- `distrib` — caller, amount distributed, resulting `reward_index`
- `claim` — claimer and amount claimed

Event payloads are defined in [events.rs](file:///Users/boufdaddy/Documents/trae_projects/axionvera-network/contracts/vault-contract/src/events.rs).

## Reward Accounting

`reward_index` is scaled by `1e18` to preserve precision.

When `distribute_rewards(amount)` is called:
- `reward_index += (amount * 1e18) / total_deposits`

When a user interacts (deposit/withdraw/claim):
- Compute `delta = reward_index - user_reward_index`
- Accrue `balance * delta / 1e18` into `user_rewards`
- Set `user_reward_index = reward_index`

This avoids iterating over depositors and keeps distribution `O(1)`.

## Extension Points (Good First Issues)

- Reward rounding strategy and dust handling
- Gas/storage read optimizations (key packing, fewer reads)
- Additional security checks (pause, caps, allowlists)
- Governance patterns (admin handover, multisig)
- Upgrade patterns compatible with Soroban best practices
