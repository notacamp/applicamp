import { defineConfig, devices } from "@playwright/test";

// One happy path against a running stack (CI: the image with deploy/compose.yml
// and the fake Campbooks of deploy/compose.e2e.yml). Nothing here starts it.
const env = process.env;

// The fake provider's issuer is a hostname the server's container resolves
// (fake-oidc); the browser maps it to the published port with
// E2E_HOST_RULES="MAP fake-oidc 127.0.0.1".
const hostRules = env["E2E_HOST_RULES"];

export default defineConfig({
  testDir: "e2e",
  testMatch: "**/*.e2e.ts",
  workers: 1,
  forbidOnly: env["CI"] !== undefined,
  retries: env["CI"] === undefined ? 0 : 1,
  reporter: [["list"], ["html", { open: "never" }]],
  use: {
    baseURL: env["E2E_BASE_URL"] ?? "http://localhost:8080",
    trace: "retain-on-failure",
    screenshot: "only-on-failure",
    ...(hostRules === undefined
      ? {}
      : { launchOptions: { args: [`--host-resolver-rules=${hostRules}`] } }),
  },
  projects: [{ name: "chromium", use: { ...devices["Desktop Chrome"] } }],
});
