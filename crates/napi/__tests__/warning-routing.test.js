"use strict";

// Integration test for NAPI warning routing (issue #1568).
//
// The `flush_warnings` helper in the Rust NAPI binding writes each render
// warning to stderr via `eprintln!("chordsketch: {w}")`.  Because
// `eprintln!` writes directly to the process's file descriptor 2, the only
// way to observe it in Node.js is to spawn a child process and capture its
// stderr.
//
// Input that reliably triggers a warning:
//   {transpose: 100} in the source + transpose option 100 on the JS side
//   → combine_transpose(100, 100) = 200 which saturates to i8::MAX (127)
//   → the renderer emits a "transpose … clamped" warning.
//
// This mirrors the Rust unit test
// `test_render_songs_with_warnings_captures_saturation_warning` in
// `crates/napi/src/lib.rs`, but exercises the full NAPI → eprintln! path
// rather than just the underlying renderer.

const { spawnSync } = require("child_process");
const fs = require("fs");
const path = require("path");

// Locate the compiled .node artifact built by the CI step that precedes this
// test.  The file name encodes the target triple, so we scan for any .node
// file in the package root rather than hard-coding a name.
function findNodeFile() {
  const dir = path.join(__dirname, "..");
  const files = fs.readdirSync(dir).filter((f) => f.endsWith(".node"));
  if (files.length !== 1) {
    throw new Error(
      `Expected exactly 1 .node file in ${dir}, found ${files.length}: ${files.join(", ")}`
    );
  }
  return path.join(dir, files[0]);
}

describe("NAPI warning routing", () => {
  let nodeFile;

  beforeAll(() => {
    nodeFile = findNodeFile();
  });

  test("renderTextWithOptions emits chordsketch:-prefixed warning to stderr on saturating transpose", () => {
    // Spawn a child process so we can capture stderr written by eprintln!.
    // A saturating transpose (source {transpose:100} + option transpose:100
    // = 200 > i8::MAX) reliably produces a "transpose ... clamped" warning.
    const result = spawnSync(
      process.execPath,
      [
        "-e",
        `
        const m = require(${JSON.stringify(nodeFile)});
        const out = m.renderTextWithOptions(
          "{title: T}\\n{transpose: 100}\\n[C]Hello",
          { transpose: 100 }
        );
        process.stdout.write(out);
      `,
      ],
      { encoding: "utf8", timeout: 10000 }
    );

    // Guard against spawn failures (result.status is null when spawnSync
    // itself fails, e.g., executable not found).
    expect(result.error).toBeUndefined();
    expect(result.status).toBe(0);
    // The rendered output must be non-empty (no regression in output).
    expect(result.stdout.trim()).toBeTruthy();
    // At least one warning must have been forwarded to stderr with the
    // "chordsketch:" prefix applied by flush_warnings.
    expect(result.stderr).toMatch(/chordsketch:/);
  });

  test("renderText with non-saturating input produces no chordsketch: warning", () => {
    // Sanity check: normal input must not produce spurious warnings.
    const result = spawnSync(
      process.execPath,
      [
        "-e",
        `
        const m = require(${JSON.stringify(nodeFile)});
        const out = m.renderText("{title: T}\\n[C]Hello");
        process.stdout.write(out);
      `,
      ],
      { encoding: "utf8", timeout: 10000 }
    );

    // Guard against spawn failures before checking exit status.
    expect(result.error).toBeUndefined();
    expect(result.status).toBe(0);
    expect(result.stdout.trim()).toBeTruthy();
    expect(result.stderr).not.toMatch(/chordsketch:/);
  });
});
