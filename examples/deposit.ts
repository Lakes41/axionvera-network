import { Keypair } from "@stellar/stellar-sdk";
import { CONFIG, signRequest, axionveraFetch } from "./common.ts";

/**
 * Example: Deposit tokens into the Axionvera Vault.
 * 
 * Usage:
 *   AXIONVERA_NODE_URL=http://localhost:8080 tsx examples/deposit.ts 100
 */
async function main() {
  const amountStr = process.argv[2] || "100";
  const userKeypair = Keypair.fromSecret(CONFIG.userSecret);
  const userAddress = userKeypair.publicKey();
  const nonce = Date.now(); // Simplified nonce for example

  console.info(`[SDK] Depositing ${amountStr} units for user ${userAddress}...`);

  const depositRequest = {
    user_address: userAddress,
    token_address: CONFIG.tokenAddress,
    amount: amountStr,
    signature: signRequest(userAddress, { amount: amountStr, nonce }),
    nonce: nonce,
    timestamp: new Date().toISOString(),
    request_id: `req_dep_${nonce}`
  };

  try {
    const response = await axionveraFetch("/v1/contract/deposit", {
      method: "POST",
      body: depositRequest,
    });

    console.log("[SDK] Deposit Response:", JSON.stringify(response, null, 2));
    if (response.success) {
      console.info(`[SDK] ✅ Successfully deposited ${amountStr} tokens. Tx: ${response.transaction_hash}`);
    } else {
      console.error(`[SDK] ❌ Deposit failed: ${response.error_message}`);
    }
  } catch (error) {
    console.error("[SDK] ❌ Error during deposit:", error.message);
  }
}

main();
