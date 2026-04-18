/**
 * Integration test: extension activation + command registration.
 *
 * This is the foundational regression gate for issue #1914. It opens a
 * `.cho` fixture (which triggers the extension's `onLanguage:chordpro`
 * activation event) and asserts that every contributed command is
 * present in `vscode.commands.getCommands(true)`. A future refactor of
 * `activate()` that propagates an error before `context.subscriptions.push`
 * runs would silently drop commands and be caught here.
 *
 * Run from `packages/vscode-extension/`:
 *
 *   npm run test:integration    # non-display environments add xvfb-run
 *
 * The runner downloads a pinned VS Code build on first run; subsequent
 * runs reuse it.
 */

import * as assert from "node:assert/strict";
import * as path from "node:path";
import * as vscode from "vscode";

/** Commands the extension contributes via `package.json#contributes.commands`. */
const CONTRIBUTED_COMMANDS = [
  "chordsketch.openPreview",
  "chordsketch.openPreviewToSide",
  "chordsketch.transposeUp",
  "chordsketch.transposeDown",
  "chordsketch.convertTo",
];

suite("extension activation", () => {
  suiteSetup(async () => {
    // Open a fixture to trigger the extension's `onLanguage:chordpro`
    // activation event. The actual rendered content does not matter —
    // we only need the ChordPro language association to fire.
    const fixtureDir = path.resolve(__dirname, "..", "..", "..", "test", "fixtures");
    const uri = vscode.Uri.file(path.join(fixtureDir, "hello.cho"));
    const doc = await vscode.workspace.openTextDocument(uri);
    await vscode.window.showTextDocument(doc);

    // Wait for activation to complete. `getExtension` returns `undefined`
    // briefly during host startup, and `activate()` is async, so poll
    // with a generous timeout before the per-test assertions run.
    const extension = vscode.extensions.getExtension("koedame.chordsketch");
    assert.ok(
      extension,
      "koedame.chordsketch extension must be installed in the extension-dev host",
    );
    if (!extension.isActive) {
      await extension.activate();
    }
    assert.ok(extension.isActive, "extension must be active after activation");
  });

  test("every contributed command is registered after activation", async () => {
    const registered = new Set(await vscode.commands.getCommands(/* filterInternal */ true));
    const missing = CONTRIBUTED_COMMANDS.filter((cmd) => !registered.has(cmd));
    assert.deepEqual(
      missing,
      [],
      `expected all contributed commands to be registered; missing: ${JSON.stringify(missing)}`,
    );
  });
});
