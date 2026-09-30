import '@/theme/tokens.css';
import '@/theme/base.css';

import { createPinia } from 'pinia';
import { createApp } from 'vue';

import App from '@/App.vue';
import { installErrorHandlers } from '@/app/errors';
import { signalReady } from '@/app/ready';
import { i18n } from '@/i18n';
import { initTheme } from '@/theme/useTheme';

// Before mounting: the theme decides the window's first painted colour, and doing this after
// the first render would show the default palette for a frame.
initTheme();

// The webview ships Chromium's own context menu (Back/Reload/Save image/Inspect) and LocalMe is
// a native desktop app exposing none of those web affordances, so the browser menu is suppressed
// everywhere. Keyboard shortcuts for clipboard and devtools keep working.
window.addEventListener('contextmenu', (event: MouseEvent): void => {
  event.preventDefault();
});

const app = createApp(App);
installErrorHandlers(app);

app.use(createPinia());
app.use(i18n);
app.mount('#app');

// After mounting: the host reveals the window once this arrives.
void signalReady();
