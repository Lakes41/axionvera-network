# Axionvera SDK Usage Examples

This directory contains practical examples for interacting with the Axionvera Network.

## Overview

The examples demonstrate the core contract interactions provided by the Axionvera Vault:
- **Deposit**: Transfer tokens from your wallet into the vault.
- **Withdraw**: Transfer tokens from the vault back to your wallet.
- **Query**: Check your current balance, pending rewards, and global vault state.

## Setup

1. **Install dependencies**:
   Ensure you have `tsx` installed globally or in your project:
   ```bash
   npm install
   ```

2. **Configure environment**:
   Create a `.env` file or export the following variables:
   ```bash
   export AXIONVERA_NODE_URL="http://localhost:8080"
   export TOKEN_ADDRESS="CAS3J7AVSSY1P3S2S3S2S3S2S3S2S3S2S3S2S3S2S3S2S3S2PURE"
   export USER_SECRET="S..." # Your Stellar Secret Key
   ```

## Running Examples

Execute the scripts using `tsx`:

### 💰 Deposit tokens
```bash
npx tsx examples/deposit.ts 150
```

### 🏧 Withdraw tokens
```bash
npx tsx examples/withdraw.ts 75
```

### 📊 Query Balance & State
```bash
npx tsx examples/query-balance.ts
```

## How it works

The scripts use standard `fetch` to communicate with the Axionvera Network Node, which acts as a gateway to the Soroban smart contracts. 
Transactions require a cryptographic signature (mocked in `common.ts` for demonstration) to prove ownership before the network node submits them to the Stellar network.
