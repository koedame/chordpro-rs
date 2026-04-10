/**
 * LSP binary resolution for the ChordSketch extension.
 *
 * Resolution order:
 *   1. User-configured path (`chordsketch.lsp.path` setting)
 *   2. `chordsketch-lsp` on the system PATH
 *   3. Platform-specific bundled binary at `server/<platform>-<arch>/chordsketch-lsp[.exe]`
 *      (only present in platform-specific VSIX packages)
 */

import * as fs from 'fs';
import * as path from 'path';
import { execFileSync } from 'child_process';

/** Maps VS Code platform identifiers to the binary directory names used in release archives. */
const PLATFORM_MAP: Record<string, Record<string, string>> = {
  linux: { x64: 'linux-x64', arm64: 'linux-arm64' },
  darwin: { x64: 'darwin-x64', arm64: 'darwin-arm64' },
  win32: { x64: 'win32-x64' },
};

/** Returns the platform-specific binary suffix (empty on Unix, `.exe` on Windows). */
function binaryExt(): string {
  return process.platform === 'win32' ? '.exe' : '';
}

/** Returns the binary name for this platform. */
export function lspBinaryName(): string {
  return `chordsketch-lsp${binaryExt()}`;
}

/**
 * Checks whether `chordsketch-lsp` is available on the system PATH.
 *
 * Uses `which` (Unix) or `where` (Windows) via a synchronous child process.
 * Returns the resolved absolute path, or `undefined` if not found.
 */
function findOnPath(): string | undefined {
  const cmd = process.platform === 'win32' ? 'where' : 'which';
  try {
    const result = execFileSync(cmd, [lspBinaryName()], { encoding: 'utf8' }).trim();
    // `which` may return multiple lines; take the first.
    const first = result.split('\n')[0].trim();
    if (first && fs.existsSync(first)) {
      return first;
    }
  } catch {
    // `which`/`where` exits non-zero when the binary is not found.
  }
  return undefined;
}

/**
 * Returns the path to the bundled LSP binary for the current platform, or
 * `undefined` if no bundled binary is available (universal VSIX package).
 */
function findBundled(extensionPath: string): string | undefined {
  const archMap = PLATFORM_MAP[process.platform];
  if (!archMap) {
    return undefined;
  }
  const dir = archMap[process.arch];
  if (!dir) {
    return undefined;
  }
  const candidate = path.join(extensionPath, 'server', dir, lspBinaryName());
  return fs.existsSync(candidate) ? candidate : undefined;
}

/**
 * Resolves the path to the `chordsketch-lsp` binary using the three-tier
 * strategy described in the module JSDoc.
 *
 * Returns the resolved path, or `undefined` if the binary cannot be found.
 */
export function resolveLspBinary(
  extensionPath: string,
  configuredPath: string,
): string | undefined {
  // Tier 1: user override.
  if (configuredPath.trim()) {
    const p = configuredPath.trim();
    return fs.existsSync(p) ? p : undefined;
  }

  // Tier 2: system PATH.
  const onPath = findOnPath();
  if (onPath) {
    return onPath;
  }

  // Tier 3: bundled binary (platform-specific VSIX only).
  return findBundled(extensionPath);
}
