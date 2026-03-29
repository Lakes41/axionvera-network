import fetch from "node-fetch";
import { Keypair } from "@stellar/stellar-sdk";

/**
 * Common configuration for Axionvera SDK examples.
 */
export const CONFIG = {
  nodeUrl: process.env.AXIONVERA_NODE_URL || "http://localhost:8080",
  userSecret: process.env.USER_SECRET || Keypair.random().secret(),
  tokenAddress: process.env.TOKEN_ADDRESS || "CAS3J7AVSSY1P3S2S3S2S3S2S3S2S3S2S3S2S3S2S3S2S3S2PURE",
};

/**
 * Helper to sign a request (Mock implementation for demonstration).
 * In a real scenario, this would use a wallet or a secure signer.
 */
export function signRequest(userAddress: string, data: any): string {
  console.log(`[SDK] Signing request for ${userAddress}...`);
  // Mocking a base64 signature
  return Buffer.from(`sig_${userAddress}_${Date.now()}`).toString("base64");
}

/**
 * Generic fetch wrapper for the Axionvera Network API.
 */
export async function axionveraFetch(endpoint: string, options: any = {}) {
  const url = `${CONFIG.nodeUrl}${endpoint}`;
  console.log(`[SDK] Calling ${url}...`);

  const response = await fetch(url, {
    method: options.method || "GET",
    headers: {
      "Content-Type": "application/json",
      ...options.headers,
    },
    body: options.body ? JSON.stringify(options.body) : undefined,
  });

  if (!response.ok) {
    const errorBody = await response.text();
    throw new Error(`API Error (${response.status}): ${errorBody}`);
  }

  return response.json();
}
