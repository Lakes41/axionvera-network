# Axionvera Network

Axionvera Network is a Soroban (Stellar) smart-contract vault and reward distribution protocol.

Users can:
- Deposit tokens into a vault
- Track per-user balances
- Withdraw funds
- Receive proportional reward distributions (via `distribute_rewards` + `claim_rewards`)

This repository is structured like a real open-source project intended for contribution programs: modular contract code, clear extension points, tests, scripts, and contribution templates.

## Repository Layout

- [contracts/vault-contract](file:///Users/boufdaddy/Documents/trae_projects/axionvera-network/contracts/vault-contract) — Soroban vault contract (Rust)
- [scripts](file:///Users/boufdaddy/Documents/trae_projects/axionvera-network/scripts) — TypeScript scripts (CLI-driven deploy/initialize)
- [tests](file:///Users/boufdaddy/Documents/trae_projects/axionvera-network/tests) — TypeScript test scaffold
- [docs](file:///Users/boufdaddy/Documents/trae_projects/axionvera-network/docs) — Architecture and contract specification

## Architecture (High Level)

The vault uses an index-based accounting model:
- `total_deposits` tracks total deposited vault shares (1:1 with deposit token units).
- `reward_index` is a cumulative “rewards-per-share” index scaled by `1e18`.
- Each user stores a `user_reward_index` snapshot and `user_rewards` accrued amount.
- `distribute_rewards(amount)` increases `reward_index` proportionally to `amount / total_deposits`.
- Users realize rewards lazily on interactions (`deposit`, `withdraw`, `claim_rewards`).

More detail:
- [docs/architecture.md](file:///Users/boufdaddy/Documents/trae_projects/axionvera-network/docs/architecture.md)
- [docs/contract-spec.md](file:///Users/boufdaddy/Documents/trae_projects/axionvera-network/docs/contract-spec.md)

## Prerequisites

- Rust (stable)
- `wasm32-unknown-unknown` target
- Soroban CLI (`soroban`)
- Node.js (18+ recommended)

## Setup

```bash
rustup target add wasm32-unknown-unknown
npm install
```

## Build (Contract)

```bash
npm run build:contracts
```

This produces:
- `target/wasm32-unknown-unknown/release/axionvera_vault_contract.wasm`

## Run Tests

Rust unit tests (recommended, fast, runs locally):

```bash
npm run test:rust
```

TypeScript tests (scaffold; integration tests are skipped unless enabled):

```bash
npm test
```

Enable integration tests only when you have a working Soroban environment:

```bash
SOROBAN_INTEGRATION=1 npm test
```

## Deploy & Initialize

These scripts use the Soroban CLI under the hood.

Deploy:

```bash
npm run build:contracts
SOROBAN_NETWORK=testnet SOROBAN_SOURCE=default npm run deploy
```

Initialize (example):

```bash
export VAULT_CONTRACT_ID="<DEPLOY_OUTPUT_CONTRACT_ID>"
export VAULT_ADMIN="<G...ADDRESS>"
export VAULT_DEPOSIT_TOKEN="<TOKEN_CONTRACT_ID>"
export VAULT_REWARD_TOKEN="<TOKEN_CONTRACT_ID>"
SOROBAN_NETWORK=testnet SOROBAN_SOURCE=default npm run initialize
```

## Contributing

- Read [CONTRIBUTING.md](file:///Users/boufdaddy/Documents/trae_projects/axionvera-network/CONTRIBUTING.md)
- See [docs/contributing-guide.md](file:///Users/boufdaddy/Documents/trae_projects/axionvera-network/docs/contributing-guide.md) for contribution areas and standards

## Security

This project is a reference-quality open-source starting point and is not audited.
Do not deploy to mainnet without a dedicated security review.
