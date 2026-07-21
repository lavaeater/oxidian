// @ts-check
const { defineConfig, devices } = require("@playwright/test");

// Oxidian is a client-only Dioxus app. Playwright boots the real web build with
// `dx serve --package web` and drives it in Chromium. There is no backend to
// stand up — tests that need repo data mock the Git-host REST API at the network
// layer (see tests/helpers.js) and seed config into localStorage, so nothing
// here touches real GitHub/GitLab or requires OAuth.

const PORT = Number(process.env.OXIDIAN_E2E_PORT || 8080);
const BASE_URL = `http://127.0.0.1:${PORT}`;

// The first `dx serve` compiles the wasm bundle, which can take a while on a
// cold target dir — give it a generous startup window.
const SERVER_TIMEOUT = 10 * 60 * 1000;

module.exports = defineConfig({
  testDir: "./tests",
  fullyParallel: true,
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 2 : 0,
  workers: process.env.CI ? 1 : undefined,
  reporter: process.env.CI ? [["html", { open: "never" }], ["list"]] : "list",
  timeout: 60 * 1000,
  expect: { timeout: 10 * 1000 },
  use: {
    baseURL: BASE_URL,
    trace: "retain-on-failure",
  },
  projects: [
    { name: "chromium", use: { ...devices["Desktop Chrome"] } },
  ],
  webServer: {
    // `dx run` builds once and serves WITHOUT file-watching/hot-patching, so no
    // "app is being rebuilt" overlay can appear mid-test (unlike `dx serve`).
    // Run from the repo root (one level up) so the package path resolves.
    command: `dx run --package web --addr 127.0.0.1 --port ${PORT} --open false --hot-reload false`,
    cwd: "..",
    url: BASE_URL,
    timeout: SERVER_TIMEOUT,
    reuseExistingServer: !process.env.CI,
    stdout: "pipe",
    stderr: "pipe",
  },
});
