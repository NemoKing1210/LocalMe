// @vitest-environment happy-dom

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { App, ComponentPublicInstance } from 'vue';

vi.mock('@/ipc', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@/ipc')>()),
  logFrontend: vi.fn(),
}));

import * as ipc from '@/ipc';

import { installErrorHandlers } from './errors';

const registered: Array<[string, EventListenerOrEventListenerObject]> = [];

function fakeApp(): App {
  return { config: {} } as unknown as App;
}

function fakeInstance(name?: string): ComponentPublicInstance {
  return { $options: name === undefined ? {} : { name } } as unknown as ComponentPublicInstance;
}

beforeEach(() => {
  registered.length = 0;
  const callThrough = window.addEventListener.bind(window);
  vi.spyOn(window, 'addEventListener').mockImplementation((type, listener, options) => {
    if (listener !== null) registered.push([type, listener]);
    callThrough(type, listener, options);
  });
  vi.spyOn(console, 'error').mockImplementation(() => {});
  vi.spyOn(console, 'warn').mockImplementation(() => {});
  vi.mocked(ipc.logFrontend).mockResolvedValue(undefined);
});

afterEach(() => {
  for (const [type, listener] of registered) window.removeEventListener(type, listener);
});

describe('installErrorHandlers', () => {
  it('installs the Vue error handler on the application', () => {
    const app = fakeApp();

    installErrorHandlers(app);

    expect(typeof app.config.errorHandler).toBe('function');
  });

  it('reports a Vue error with its component trace through the log', () => {
    const app = fakeApp();
    installErrorHandlers(app);

    app.config.errorHandler!(new Error('kaboom'), fakeInstance('ChatView'), 'render');

    expect(ipc.logFrontend).toHaveBeenCalledWith(
      'error',
      'vue: Error: kaboom',
      expect.stringContaining('render in ChatView'),
    );
    expect(console.error).toHaveBeenCalledWith(
      expect.stringContaining('[localme] vue: Error: kaboom (render in ChatView)'),
      expect.any(Error),
    );
  });

  it('names the component "anonymous" when it has no name and uses the info alone without one', () => {
    const app = fakeApp();
    installErrorHandlers(app);
    const handler = app.config.errorHandler!;

    handler(new Error('first'), fakeInstance(), 'setup');
    expect(ipc.logFrontend).toHaveBeenLastCalledWith(
      'error',
      'vue: Error: first',
      expect.stringContaining('setup in anonymous'),
    );

    handler(new Error('second'), null, 'setup');
    const context = vi.mocked(ipc.logFrontend).mock.calls.at(-1)?.[2];
    expect(context).not.toContain(' in ');
  });

  it('formats an Error, a string, an object and a circular object', () => {
    const app = fakeApp();
    installErrorHandlers(app);
    const handler = app.config.errorHandler!;
    const circular: Record<string, unknown> = {};
    circular['self'] = circular;

    const cases: Array<readonly [unknown, string]> = [
      [new TypeError('bad type'), 'vue: TypeError: bad type'],
      ['plain failure', 'vue: plain failure'],
      [{ code: 42 }, 'vue: {"code":42}'],
      [circular, 'vue: [object Object]'],
    ];

    for (const [cause, message] of cases) {
      handler(cause, null, 'render');
      expect(ipc.logFrontend).toHaveBeenLastCalledWith('error', message, expect.any(String));
    }
  });

  it('reports an unhandled rejection with a scope prefix', () => {
    installErrorHandlers(fakeApp());
    const reason = new Error('nope');

    window.dispatchEvent(Object.assign(new Event('unhandledrejection'), { reason }));

    expect(ipc.logFrontend).toHaveBeenCalledWith(
      'error',
      'unhandled-rejection: Error: nope',
      expect.any(String),
    );
  });

  it('describes a non-Error rejection by value', () => {
    installErrorHandlers(fakeApp());

    window.dispatchEvent(
      Object.assign(new Event('unhandledrejection'), { reason: { code: 'boom' } }),
    );

    expect(ipc.logFrontend).toHaveBeenCalledWith(
      'error',
      'unhandled-rejection: {"code":"boom"}',
      expect.any(String),
    );
  });

  it('reports a window error using its message', () => {
    installErrorHandlers(fakeApp());

    window.dispatchEvent(
      Object.assign(new Event('error'), { message: 'illegal invocation', error: new Error('x') }),
    );

    expect(ipc.logFrontend).toHaveBeenCalledWith(
      'error',
      'window: illegal invocation',
      expect.any(String),
    );
  });

  it('falls back to the error itself when the window event carries no message', () => {
    installErrorHandlers(fakeApp());

    window.dispatchEvent(
      Object.assign(new Event('error'), { message: '', error: new Error('deep failure') }),
    );

    expect(ipc.logFrontend).toHaveBeenCalledWith(
      'error',
      'window: Error: deep failure',
      expect.any(String),
    );
  });

  it('swallows a rejected log write so a handled error never becomes unhandled', async () => {
    const app = fakeApp();
    vi.mocked(ipc.logFrontend).mockRejectedValue(new Error('the host is gone'));
    installErrorHandlers(app);

    // The handler must not rethrow; awaiting the microtask queue would surface the rejection as
    // an unhandled one if the `.catch` were missing.
    app.config.errorHandler!(new Error('reported'), null, 'render');
    await Promise.resolve();
    await Promise.resolve();

    expect(ipc.logFrontend).toHaveBeenCalledTimes(1);
  });
});
