/**
 * Mounting a view the way the application does: one Pinia, one router, the i18n plugin and the
 * same route names. A test may still pass its own Pinia or router when it needs to observe them
 * (a guard, a store filled before mount).
 */

import { flushPromises, mount, type GlobalMountOptions, type VueWrapper } from '@vue/test-utils';
import { createPinia, setActivePinia, type Pinia } from 'pinia';
import { defineComponent, type Component } from 'vue';
import { createMemoryHistory, createRouter, type Router } from 'vue-router';

import { ROUTE } from '@/app/routes';
import { i18n } from '@/i18n';

export { flushPromises };

const BlankRoute = defineComponent({ name: 'BlankRoute', template: '<div />' });

/**
 * The real route table, with the lazy pages replaced by an empty component: a test about the
 * chat view mounts the chat view, not whatever the router would have rendered for `/chat`.
 */
export function createTestRouter(): Router {
  return createRouter({
    history: createMemoryHistory(),
    routes: [
      { path: '/', redirect: { name: ROUTE.chat } },
      { path: '/chat/:deviceId?', name: ROUTE.chat, component: BlankRoute },
      { path: '/settings', name: ROUTE.settings, component: BlankRoute },
      { path: '/:pathMatch(.*)*', redirect: { name: ROUTE.chat } },
    ],
  });
}

export interface ViewMountOptions {
  /** Props for the root component. */
  readonly props?: Record<string, unknown>;
  /** Where the router starts; the default `/` lands on the chat route. */
  readonly route?: string;
  /** Pass an existing Pinia to read the stores the component filled. */
  readonly pinia?: Pinia;
  /** Pass an existing router to assert on navigation. */
  readonly router?: Router;
  /** Components to replace; `{ RouterView: true }` is the usual one. */
  readonly stubs?: Record<string, boolean | Component>;
  /** Attach to the document when the component measures or scrolls real layout. */
  readonly attachTo?: HTMLElement;
}

export async function mountView(
  component: Component,
  options: ViewMountOptions = {},
): Promise<VueWrapper> {
  const pinia = options.pinia ?? createPinia();
  // Stores created outside a component (in a test body) must resolve to the same Pinia the
  // component will use, and `setActivePinia` is what makes that true.
  setActivePinia(pinia);

  const router = options.router ?? createTestRouter();
  await router.push(options.route ?? '/');
  await router.isReady();

  const global: GlobalMountOptions = { plugins: [pinia, router, i18n] };
  if (options.stubs !== undefined) global.stubs = options.stubs;

  return mount(component, {
    ...(options.props === undefined ? {} : { props: options.props }),
    ...(options.attachTo === undefined ? {} : { attachTo: options.attachTo }),
    global,
  });
}
