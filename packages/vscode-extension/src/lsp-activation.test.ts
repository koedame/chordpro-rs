/**
 * Unit tests for the VS Code-free LSP activation helpers.
 *
 * Run with:
 *   npm test
 *   node --experimental-transform-types --test src/lsp-activation.test.ts
 */

import { test } from 'node:test';
import assert from 'node:assert/strict';
// Use .ts extension — executed directly by Node with --experimental-transform-types.
import {
    startLspClientSafely,
    tryStartLanguageClient,
    type StartableClient,
} from './lsp-activation.ts';

// --- startLspClientSafely ---

test('startLspClientSafely: does not log or notify when start resolves', async () => {
  const logs: string[] = [];
  const notifications: string[] = [];
  await startLspClientSafely({
    start: async () => {},
    log: (m) => logs.push(m),
    notify: (m) => notifications.push(m),
  });
  assert.deepEqual(logs, []);
  assert.deepEqual(notifications, []);
});

test('startLspClientSafely: logs Error detail and notifies when start throws Error', async () => {
  const logs: string[] = [];
  const notifications: string[] = [];
  await startLspClientSafely({
    start: async () => {
      throw new Error('unsupported position encoding (utf-8)');
    },
    log: (m) => logs.push(m),
    notify: (m) => notifications.push(m),
  });
  // Two diagnostic lines: the failure and the "commands remain" hint.
  assert.equal(logs.length, 2);
  assert.ok(logs[0].includes('unsupported position encoding'));
  assert.ok(logs[1].includes('Preview and transpose/convert commands'));
  // Single info notification.
  assert.equal(notifications.length, 1);
  assert.ok(notifications[0].includes('LSP failed to start'));
});

test('startLspClientSafely: handles non-Error rejection values', async () => {
  const logs: string[] = [];
  await startLspClientSafely({
    start: async () => {
      // eslint-disable-next-line @typescript-eslint/no-throw-literal
      throw 'spawn EACCES';
    },
    log: (m) => logs.push(m),
    notify: () => {},
  });
  assert.ok(logs[0].includes('spawn EACCES'));
});

test('startLspClientSafely: never propagates the thrown error', async () => {
  // The whole point: an unhandled throw inside activate() would skip every
  // subsequent context.subscriptions.push. Verify the helper absorbs it.
  await assert.doesNotReject(async () => {
    await startLspClientSafely({
      start: async () => {
        throw new Error('boom');
      },
      log: () => {},
      notify: () => {},
    });
  });
});

// --- tryStartLanguageClient ---

test('tryStartLanguageClient: assigns the client on successful start', async () => {
  let published: StartableClient | undefined;
  const client: StartableClient = {
    start: async () => {},
    dispose: () => {},
  };
  await tryStartLanguageClient(client, (c) => {
    published = c;
  });
  assert.strictEqual(published, client);
});

test('tryStartLanguageClient: resets the published client to undefined when start throws', async () => {
  // Simulate the previous-successful-start state: setClient holds a stale
  // reference from an earlier successful attempt.
  let published: StartableClient | undefined = {
    start: async () => {},
    dispose: () => {},
  };
  const failingClient: StartableClient = {
    start: async () => {
      throw new Error('initialize rejected');
    },
    dispose: () => {},
  };
  await assert.rejects(
    tryStartLanguageClient(failingClient, (c) => {
      published = c;
    }),
    /initialize rejected/,
  );
  // Must not leak the half-initialized client.
  assert.strictEqual(published, undefined);
});

test('tryStartLanguageClient: disposes the failed client', async () => {
  let disposed = false;
  const failingClient: StartableClient = {
    start: async () => {
      throw new Error('bang');
    },
    dispose: () => {
      disposed = true;
    },
  };
  await assert.rejects(
    tryStartLanguageClient(failingClient, () => {}),
    /bang/,
  );
  assert.equal(disposed, true);
});

test('tryStartLanguageClient: swallows dispose failures and surfaces the original error', async () => {
  const failingClient: StartableClient = {
    start: async () => {
      throw new Error('primary failure');
    },
    dispose: () => {
      throw new Error('secondary dispose failure');
    },
  };
  // The primary error must reach the caller — dispose-failure noise is suppressed.
  await assert.rejects(
    tryStartLanguageClient(failingClient, () => {}),
    /primary failure/,
  );
});

test('tryStartLanguageClient: re-throws the original error after failure', async () => {
  const original = new Error('initialize rejected');
  const failingClient: StartableClient = {
    start: async () => {
      throw original;
    },
    dispose: () => {},
  };
  let caught: unknown;
  try {
    await tryStartLanguageClient(failingClient, () => {});
  } catch (err) {
    caught = err;
  }
  // Identity preserved — the exact Error object reaches activate().
  assert.strictEqual(caught, original);
});
