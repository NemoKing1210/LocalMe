/**
 * Startup handshake with the host.
 *
 * The window is created hidden so that the first thing the user sees is a fully themed
 * frame, not a white one that turns dark a moment later. The front end reports readiness
 * once Vue has mounted, and the host reveals the window then. The host also has a timeout
 * of its own, so a front-end failure cannot leave an invisible application running.
 */
import { emit } from '@tauri-apps/api/event';

/** Event name shared with `src-tauri/src/window.rs`. */
export const MAIN_WINDOW_READY_EVENT = 'localme://ready';

/**
 * Tells the host that the first frame is painted.
 *
 * Failures are logged and swallowed: running the front end in a plain browser (which is how
 * the UI is developed) has no host to answer, and that must not be an error.
 */
export async function signalReady(): Promise<void> {
  try {
    await emit(MAIN_WINDOW_READY_EVENT);
  } catch (error) {
    console.warn('[localme] host did not acknowledge readiness', error);
  }
}
