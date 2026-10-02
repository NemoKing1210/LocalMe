import { createRouter, createWebHashHistory, type RouteRecordRaw } from 'vue-router';

import { ROUTE } from './routes';
import ShellView from './ShellView.vue';

const routes: RouteRecordRaw[] = [
  {
    path: '/',
    component: ShellView,
    children: [
      { path: '', redirect: { name: ROUTE.chat } },
      {
        path: 'chat/:deviceId?',
        name: ROUTE.chat,
        component: () => import('@/features/chat/ChatView.vue'),
      },
      {
        path: 'settings',
        name: ROUTE.settings,
        component: () => import('@/features/settings/SettingsView.vue'),
      },
      { path: ':pathMatch(.*)*', redirect: { name: ROUTE.chat } },
    ],
  },
];

export const router = createRouter({
  history: createWebHashHistory(),
  routes,
});
