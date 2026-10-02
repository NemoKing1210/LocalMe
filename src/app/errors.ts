import type { App, ComponentPublicInstance } from 'vue';

import * as ipc from '@/ipc';

export interface ErrorReport {
  readonly scope: 'vue' | 'unhandled-rejection' | 'window';
  readonly message: string;
  readonly componentTrace?: string;
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

function stackFrames(value: unknown, limit = 6): string | undefined {
  if (!(value instanceof Error) || typeof value.stack !== 'string') return undefined;
  const frames = value.stack
    .split('\n')
    .slice(1, limit + 1)
    .map((line) => line.trim().replace(/^at\s+/, ''));
  return frames.length === 0 ? undefined : frames.join(' ← ');
}

function report(report: ErrorReport): void {
  const context = [report.componentTrace, stackFrames(report.cause)]
    .filter((part): part is string => part !== undefined && part.length > 0)
    .join(' | ');
  const detail = report.componentTrace ? ` (${report.componentTrace})` : '';
  console.error(`[localme] ${report.scope}: ${report.message}${detail}`, report.cause);

  // The same sentence, in the host's daily file. A rejection here means the host is gone or was
  // never there (a plain-browser dev run), which is not itself worth reporting: it would
  // recurse, and the console line above is already on screen.
  void ipc
    .logFrontend('error', `${report.scope}: ${report.message}`, context)
    .catch(() => undefined);
}

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
