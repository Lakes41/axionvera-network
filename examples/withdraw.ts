import { Keypair } from "@stellar/stellar-sdk";
import { CONFIG, signRequest, axionveraFetch } from "./common.ts";

/**
 * Example: Withdraw tokens from the Axionvera Vault.
 * 
 * Usage:
 *   AXIONVERA_NODE_URL=http://localhost:8080 tsx examples/withdraw.ts 50
 */
async function main() {
  const amountStr = process.argv[2] || "50";
  const userKeypair = Keypair.fromSecret(CONFIG.userSecret);
  const userAddress = userKeypair.publicKey();
  const nonce = Date.now();

  console.info(`[SDK] Withdrawing ${amountStr} units for user ${userAddress}...`);

  const withdrawRequest = {
    user_address: userAddress,
    token_address: CONFIG.tokenAddress,
    amount: amountStr,
    signature: signRequest(userAddress, { amount: amountStr, nonce }),
    nonce: nonce,
    timestamp: new Date().toISOString(),
    request_id: `req_with_${nonce}`
  };

  try {
    const response = await axionveraFetch("/v1/contract/withdraw", {
      method: "POST",
      body: withdrawRequest,
    });

    console.log("[SDK] Withdraw Response:", JSON.stringify(response, null, 2));
    if (response.success) {
      console.info(`[SDK] ✅ Successfully withdrew ${amountStr} tokens. Tx: ${response.transaction_hash}`);
    } else {
      console.error(`[SDK] ❌ Withdrawal failed: ${response.error_message}`);
    }
  } catch (error) {
    console.error("[SDK] ❌ Error during withdrawal:", error.message);
  }
}

main();
