/**
 * Application-wide error handling.
 *
 * Two classes of failure reach here and they are handled differently:
 *
 * * a rendering or lifecycle error inside a component — Vue has already torn the subtree
 *   down, so all that is left is to report it rather than let it become an unhandled
 *   rejection in the console;
 * * an unhandled promise rejection from an IPC call that a component forgot to `await` —
 *   common enough in event handlers that it deserves a report instead of silence.
 *
 * Reporting means: a bounded, structured `console.error` with the component trace, and a
 * single call site to grow later (a toast, a "report" link). Nothing here swallows an error.
 */
import type { App, ComponentPublicInstance } from 'vue';

/** What the reporter receives, kept structured so a future sink can filter on it. */
export interface ErrorReport {
  /** Where the failure surfaced. */
  readonly scope: 'vue' | 'unhandled-rejection' | 'window';
  /** The thrown value, converted to something printable. */
  readonly message: string;
  /** Vue's component trace, when the failure came from a component. */
  readonly componentTrace?: string;
  /** The original value, for the debugger. */
  readonly cause: unknown;
}

function describe(value: unknown): string {
  if (value instanceof Error) return `${value.name}: ${value.message}`;
  if (typeof value === 'string') return value;
  try {
    return JSON.stringify(value) ?? String(value);
  } catch {
    return String(value);
  }
}

function report(report: ErrorReport): void {
  const detail = report.componentTrace ? ` (${report.componentTrace})` : '';
  console.error(`[localme] ${report.scope}: ${report.message}${detail}`, report.cause);
}

/** Installs the Vue and window-level handlers. */
export function installErrorHandlers(app: App): void {
  app.config.errorHandler = (
    error: unknown,
    instance: ComponentPublicInstance | null,
    info: string,
  ): void => {
    report({
      scope: 'vue',
      message: describe(error),
      componentTrace: instance ? `${info} in ${instance.$options.name ?? 'anonymous'}` : info,
      cause: error,
    });
  };

  window.addEventListener('unhandledrejection', (event: PromiseRejectionEvent): void => {
    report({
      scope: 'unhandled-rejection',
      message: describe(event.reason),
      cause: event.reason,
    });
  });

  window.addEventListener('error', (event: ErrorEvent): void => {
    report({
      scope: 'window',
      message: event.message || describe(event.error),
      cause: event.error,
    });
  });
}
