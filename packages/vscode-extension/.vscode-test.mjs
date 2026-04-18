// Configuration for `@vscode/test-cli`. Runs the extension-host
// integration tests against a downloaded VS Code build.
//
// Tests are authored in TypeScript under `test/integration/` and compiled
// to `out-test/test/integration/` via `tsconfig.integration.json`. The
// `files` pattern below points at the compiled JS, not the TS sources.
//
// See the test-electron + test-cli guide:
//   https://code.visualstudio.com/api/working-with-extensions/testing-extension
//
// Tests run against the `stable` channel — the most recent release that
// real users have installed. A second matrix entry pinned to the
// `engines.vscode` floor (1.85.0) is a planned follow-up so both ceiling
// and floor of supported versions are exercised (see issue #1918 for the
// phased plan; the floor is deliberately deferred to keep the first CI
// rollout fast).

import { defineConfig } from "@vscode/test-cli";

export default defineConfig({
  label: "integrationTests",
  files: "out-test/test/integration/**/*.test.js",
  version: "stable",
  // Workspace is empty by default; tests that open fixture files do so
  // programmatically via `vscode.workspace.openTextDocument`.
  workspaceFolder: "./test/fixtures",
  mocha: {
    ui: "tdd",
    timeout: 60_000, // VS Code startup + extension activation can be slow on cold caches
    color: true,
  },
});
