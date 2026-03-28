# Axionvera Network

Axionvera Network is a Soroban (Stellar) vault and reward distribution project. This repository contains:

- a Soroban smart contract for deposits, withdrawals, and rewards
- a Rust network node service around the contract
- deployment scripts and infrastructure code
- tests and contributor documentation

## What The Contract Does

The vault contract supports four main user actions:

1. Deposit a token into the vault.
2. Withdraw deposited funds later.
3. Receive proportional rewards when an admin distributes them.
4. Claim accrued rewards.

The reward model is index-based, which means the contract updates a global reward index instead of looping through every user during a distribution.

## Start Here

If you are onboarding as a contributor, these are the best first reads:

- [docs/contract-spec.md](/c:/Users/ADMIN/Desktop/remmy-drips/axionvera-network/docs/contract-spec.md)
- [docs/contract-storage.md](/c:/Users/ADMIN/Desktop/remmy-drips/axionvera-network/docs/contract-storage.md)
- [contracts/vault-contract/src/lib.rs](/c:/Users/ADMIN/Desktop/remmy-drips/axionvera-network/contracts/vault-contract/src/lib.rs)
- [contracts/vault-contract/src/storage.rs](/c:/Users/ADMIN/Desktop/remmy-drips/axionvera-network/contracts/vault-contract/src/storage.rs)
- [ARCHITECTURE.md](/c:/Users/ADMIN/Desktop/remmy-drips/axionvera-network/ARCHITECTURE.md)

## Contract At A Glance

Key ideas:

- `total_deposits` tracks the total deposited amount.
- `reward_index` tracks cumulative rewards per deposited unit.
- each user stores a personal reward index snapshot and accrued rewards.
- rewards are realized lazily on user interaction.

Core public functions:

- `initialize(admin, deposit_token, reward_token)`
- `deposit(from, amount)`
- `withdraw(to, amount)`
- `distribute_rewards(amount)`
- `claim_rewards(user)`
- `balance(user)`
- `total_deposits()`
- `reward_index()`
- `pending_rewards(user)`

## Storage Overview

The contract stores both global and per-user state.

Global keys:

- initialization flag
- admin address
- deposit token address
- reward token address
- total deposits
- reward index

Per-user keys:

- deposited balance
- last synced reward index
- accrued but unclaimed rewards

Read the full walkthrough in [docs/contract-storage.md](/c:/Users/ADMIN/Desktop/remmy-drips/axionvera-network/docs/contract-storage.md).

## Example Flow

```rust
vault.initialize(&admin, &deposit_token_id, &reward_token_id);

vault.deposit(&alice, &100);
vault.deposit(&bob, &300);

vault.distribute_rewards(&400);

assert_eq!(vault.pending_rewards(&alice), 100);
assert_eq!(vault.pending_rewards(&bob), 300);

assert_eq!(vault.claim_rewards(&alice), 100);
assert_eq!(vault.claim_rewards(&bob), 300);
```

## Repository Layout

- [contracts/vault-contract](/c:/Users/ADMIN/Desktop/remmy-drips/axionvera-network/contracts/vault-contract) - Soroban vault contract in Rust
- [network-node](/c:/Users/ADMIN/Desktop/remmy-drips/axionvera-network/network-node) - network service and API layer
- [docs](/c:/Users/ADMIN/Desktop/remmy-drips/axionvera-network/docs) - contract and architecture documentation
- [scripts](/c:/Users/ADMIN/Desktop/remmy-drips/axionvera-network/scripts) - deployment and helper scripts
- [tests](/c:/Users/ADMIN/Desktop/remmy-drips/axionvera-network/tests) - integration and TypeScript tests
- [terraform](/c:/Users/ADMIN/Desktop/remmy-drips/axionvera-network/terraform) - infrastructure as code

## Local Setup

Prerequisites:

- Rust stable
- `wasm32-unknown-unknown` target
- Soroban CLI
- Node.js 18+

Basic setup:

```bash
git clone https://github.com/your-org/axionvera-network.git
cd axionvera-network
npm install
rustup target add wasm32-unknown-unknown
```

Build the contract:

```bash
npm run build:contracts
```

## Testing

Contract tests:

```bash
cargo test -p axionvera-vault-contract
```

Project-level shortcuts:

```bash
npm run test:rust
npm test
```

## Contributor Notes

- Start with the tests if you want executable examples of the contract behavior.
- Read storage docs before changing accounting logic.
- If you touch deposits, withdrawals, or rewards, verify both state changes and emitted events.

## More Documentation

- [docs/contract-spec.md](/c:/Users/ADMIN/Desktop/remmy-drips/axionvera-network/docs/contract-spec.md)
- [docs/contract-storage.md](/c:/Users/ADMIN/Desktop/remmy-drips/axionvera-network/docs/contract-storage.md)
- [docs/architecture.md](/c:/Users/ADMIN/Desktop/remmy-drips/axionvera-network/docs/architecture.md)
- [ARCHITECTURE.md](/c:/Users/ADMIN/Desktop/remmy-drips/axionvera-network/ARCHITECTURE.md)
- [CONTRIBUTING.md](/c:/Users/ADMIN/Desktop/remmy-drips/axionvera-network/CONTRIBUTING.md)
