import { execFileSync } from "node:child_process";

type InitializeConfig = {
  network: string;
  source: string;
  contractId: string;
  admin: string;
  depositToken: string;
  rewardToken: string;
};

function envOrThrow(name: string): string {
  const v = process.env[name];
  if (!v) throw new Error(`Missing required env var: ${name}`);
  return v;
}

function readConfigFromEnv(): InitializeConfig {
  return {
    network: process.env.SOROBAN_NETWORK ?? "testnet",
    source: process.env.SOROBAN_SOURCE ?? "default",
    contractId: envOrThrow("VAULT_CONTRACT_ID"),
    admin: envOrThrow("VAULT_ADMIN"),
    depositToken: envOrThrow("VAULT_DEPOSIT_TOKEN"),
    rewardToken: envOrThrow("VAULT_REWARD_TOKEN")
  };
}

function initialize(cfg: InitializeConfig): string {
  const stdout = execFileSync(
    "soroban",
    [
      "contract",
      "invoke",
      "--id",
      cfg.contractId,
      "--source",
      cfg.source,
      "--network",
      cfg.network,
      "--",
      "initialize",
      "--admin",
      cfg.admin,
      "--deposit_token",
      cfg.depositToken,
      "--reward_token",
      cfg.rewardToken
    ],
    { encoding: "utf8", stdio: ["ignore", "pipe", "inherit"] }
  );

  return stdout.trim();
}

const cfg = readConfigFromEnv();
const out = initialize(cfg);
process.stdout.write(`${out}\n`);
