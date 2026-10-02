// @vitest-environment happy-dom

import { describe, expect, it } from 'vitest';
import type { Router } from 'vue-router';

import { createTestRouter } from '@/test/mount';

import { router as appRouter } from './router';
import { ROUTE } from './routes';

async function routerAt(path: string): Promise<Router> {
  const router = createTestRouter();
  await router.push(path);
  await router.isReady();
  return router;
}

describe('router', () => {
  it('redirects the root path to the chat route', async () => {
    const router = await routerAt('/');
    expect(router.currentRoute.value.name).toBe(ROUTE.chat);
  });

  it('resolves /chat/<id> to the chat route with the device id as a param', async () => {
    const router = await routerAt('/chat/peer-42');

    expect(router.currentRoute.value.name).toBe(ROUTE.chat);
    expect(router.currentRoute.value.params.deviceId).toBe('peer-42');
  });

  it('resolves /chat without an id to the chat route with no param', async () => {
    const router = await routerAt('/chat');

    expect(router.currentRoute.value.name).toBe(ROUTE.chat);
    // An absent optional param is reported as an empty string, not undefined.
    expect(router.currentRoute.value.params.deviceId).toBe('');
  });

  it('resolves /settings to the settings route', async () => {
    const router = await routerAt('/settings');
    expect(router.currentRoute.value.name).toBe(ROUTE.settings);
  });

  it('falls back to the chat route for an unknown path', async () => {
    const router = await routerAt('/does/not/exist');
    expect(router.currentRoute.value.name).toBe(ROUTE.chat);
  });
});

describe('the application router', () => {
  // A record's `default` is the lazy `import()` until navigation resolves it; asserting it is no
  // longer a function is what proves the page chunk actually loaded for that route.
  function resolvedDefault(index: number): unknown {
    const record = appRouter.currentRoute.value.matched[index];
    return record?.components?.default;
  }

  it('loads the chat page for the chat route', async () => {
    await appRouter.push('/chat/peer-1');
    await appRouter.isReady();

    expect(appRouter.currentRoute.value.name).toBe(ROUTE.chat);
    expect(typeof resolvedDefault(1)).not.toBe('function');
    expect(resolvedDefault(1)).toBeTruthy();
  });

  it('loads the settings page for the settings route', async () => {
    await appRouter.push('/settings');
    await appRouter.isReady();

    expect(appRouter.currentRoute.value.name).toBe(ROUTE.settings);
    expect(typeof resolvedDefault(1)).not.toBe('function');
    expect(resolvedDefault(1)).toBeTruthy();
  });
});
