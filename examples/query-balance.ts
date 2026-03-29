import { Keypair } from "@stellar/stellar-sdk";
import { CONFIG, axionveraFetch } from "./common.ts";

/**
 * Example: Query user balance and rewards from the Axionvera Network.
 * 
 * Usage:
 *   AXIONVERA_NODE_URL=http://localhost:8080 tsx examples/query-balance.ts
 */
async function main() {
  const userKeypair = Keypair.fromSecret(CONFIG.userSecret);
  const userAddress = userKeypair.publicKey();

  console.info(`[SDK] Querying state for user ${userAddress}...`);

  try {
    // 1. Get user balance and pending rewards
    const balanceUrl = `/v1/query/balance?user_address=${userAddress}&token_address=${CONFIG.tokenAddress}`;
    const balanceResponse = await axionveraFetch(balanceUrl);

    console.log("[SDK] Balance Response:", JSON.stringify(balanceResponse, null, 2));
    if (balanceResponse.balance) {
      console.info(`[SDK] 💰 User Balance: ${balanceResponse.balance}`);
      console.info(`[SDK] 🎁 Pending Rewards: ${balanceResponse.pending_rewards}`);
    }

    // 2. Get global contract state
    const stateResponse = await axionveraFetch(`/v1/query/contract-state?contract_address=${CONFIG.tokenAddress}`);
    console.log("[SDK] Contract State:", JSON.stringify(stateResponse, null, 2));
    console.info(`[SDK] 🏦 Total Deposits in Vault: ${stateResponse.total_deposits}`);

  } catch (error) {
    console.error("[SDK] ❌ Error during query:", error.message);
  }
}

main();
