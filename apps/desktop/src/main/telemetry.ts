import * as Sentry from "@sentry/electron/main";
import type { DesktopSettings } from "../shared/types.js";
import { redactSentryEvent } from "../shared/telemetry-redaction.js";

const SENTRY_IPC_NAMESPACE = "mxr-sentry";

let initialized = false;

export function configureMainTelemetry(
  settings: DesktopSettings,
  options: { dsn?: string; version: string; environment?: string },
): void {
  if (!settings.telemetry.sentryEnabled || !options.dsn) {
    if (initialized) {
      void Sentry.close(2_000).finally(() => {
        initialized = false;
      });
    }
    return;
  }

  if (initialized) {
    return;
  }

  Sentry.init({
    dsn: options.dsn,
    release: `mxr-desktop@${options.version}`,
    environment: options.environment ?? "production",
    ipcNamespace: SENTRY_IPC_NAMESPACE,
    sendDefaultPii: false,
    tracesSampleRate: 0,
    attachScreenshot: false,
    beforeSend: (event) => redactSentryEvent(event),
  });
  initialized = true;
}
