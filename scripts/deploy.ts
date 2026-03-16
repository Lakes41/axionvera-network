import { execFileSync } from "node:child_process";
import path from "node:path";

type DeployConfig = {
  network: string;
  source: string;
  wasmPath: string;
};

function readConfigFromEnv(): DeployConfig {
  const network = process.env.SOROBAN_NETWORK ?? "testnet";
  const source = process.env.SOROBAN_SOURCE ?? "default";
  const wasmPath =
    process.env.VAULT_WASM ??
    path.resolve(
      "target/wasm32-unknown-unknown/release/axionvera_vault_contract.wasm"
    );

  return { network, source, wasmPath };
}

function deploy({ network, source, wasmPath }: DeployConfig): string {
  const stdout = execFileSync(
    "soroban",
    ["contract", "deploy", "--wasm", wasmPath, "--source", source, "--network", network],
    { encoding: "utf8", stdio: ["ignore", "pipe", "inherit"] }
  );

  const contractId = stdout.trim().split(/\s+/).at(-1);
  if (!contractId) {
    throw new Error(`Failed to parse contract ID from output: ${stdout}`);
  }
  return contractId;
}

const cfg = readConfigFromEnv();
const id = deploy(cfg);
process.stdout.write(`${id}\n`);
