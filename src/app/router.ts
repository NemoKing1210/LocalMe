/**
 * The page table.
 *
 * LocalMe is a two-pane desktop application, and the two panes are two levels of the same
 * route tree rather than two components that happen to be on screen together:
 *
 * * the parent route is the shell — the people list, which is always there in a wide window;
 * * a child route is what the detail pane shows, so "a conversation" and "settings" are pages
 *   with their own address, their own back behaviour and their own transition, instead of a
 *   screen flag that the shell reads to decide what to draw.
 *
 * That is why settings is a sibling of a conversation and not an overlay: pressing the system
 * back gesture, or the back button in the settings bar, does exactly what the address says.
 *
 * Hash history, not HTML5 history: a packaged Tauri build serves the front end from a custom
 * protocol where a path is a file name, and `#/settings` is the one form that works in the dev
 * server and in the bundle without a server-side rewrite.
 */
import { createRouter, createWebHashHistory, type RouteRecordRaw } from 'vue-router';

import { ROUTE } from './routes';
import ShellView from './ShellView.vue';

const routes: RouteRecordRaw[] = [
  {
    path: '/',
    component: ShellView,
    children: [
      // `/` and `/chat` are the same page: the shell with an empty detail pane.
      { path: '', redirect: { name: ROUTE.chat } },
      {
        path: 'chat/:deviceId?',
        name: ROUTE.chat,
        // Loaded lazily: a user who never opens settings never downloads it, and the two pages
        // do not have to be reachable from each other's module graph.
        component: () => import('@/features/chat/ChatView.vue'),
      },
      {
        path: 'settings',
        name: ROUTE.settings,
        component: () => import('@/features/settings/SettingsView.vue'),
      },
      // A stale address — a forgotten device, a typo — lands on the list rather than on an
      // empty window.
      { path: ':pathMatch(.*)*', redirect: { name: ROUTE.chat } },
    ],
  },
];

export const router = createRouter({
  history: createWebHashHistory(),
  routes,
});
