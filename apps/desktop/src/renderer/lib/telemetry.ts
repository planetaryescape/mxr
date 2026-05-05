import type { DesktopSettings } from "../../shared/types";
import { redactSentryEvent } from "../../shared/telemetry-redaction";

const SENTRY_IPC_NAMESPACE = "mxr-sentry";

let initialized = false;
let desiredEnabled = false;

export function configureRendererTelemetry(settings: DesktopSettings): void {
  if (import.meta.env.MODE === "test") {
    return;
  }

  const dsn = import.meta.env.VITE_MXR_DESKTOP_SENTRY_DSN as string | undefined;
  desiredEnabled = Boolean(settings.telemetry.sentryEnabled && dsn);
  if (!desiredEnabled) {
    if (initialized) {
      void import("@sentry/core").then(({ close }) =>
        close(2_000).finally(() => {
          initialized = false;
        }),
      );
    }
    return;
  }

  if (initialized) {
    return;
  }

  void import("@sentry/electron/renderer").then((Sentry) => {
    if (!desiredEnabled || initialized) {
      return;
    }
    Sentry.init({
      dsn,
      ipcNamespace: SENTRY_IPC_NAMESPACE,
      sendDefaultPii: false,
      tracesSampleRate: 0,
      beforeSend: (event) => redactSentryEvent(event),
    });
    initialized = true;
  });
}
