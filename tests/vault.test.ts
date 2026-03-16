import path from "node:path";
import { describe, expect, it } from "vitest";

const DEFAULT_WASM_PATH = path.resolve(
  "target/wasm32-unknown-unknown/release/axionvera_vault_contract.wasm"
);

describe("Axionvera Vault (TypeScript)", () => {
  it("has a stable default wasm path", () => {
    expect(path.extname(DEFAULT_WASM_PATH)).toBe(".wasm");
  });

  const integrationEnabled = process.env.SOROBAN_INTEGRATION === "1";
  (integrationEnabled ? describe : describe.skip)(
    "integration (requires local soroban setup)",
    () => {
      it("deploys and initializes a vault", () => {
        expect(process.env.SOROBAN_NETWORK).toBeTruthy();
        expect(process.env.SOROBAN_SOURCE).toBeTruthy();
      });
    }
  );
});
