import { EXPECTED_IPC_PROTOCOL_VERSION } from "@/lib/protocolCompatibility";

export const OFFLINE_MAIL_CACHE = `mxr-opened-mail-v1-ipc${EXPECTED_IPC_PROTOCOL_VERSION}`;
export const OFFLINE_HEADER = "x-mxr-offline";

/** The proxy's network gate is distinct from a mailbox permission rejection. */
export function isUnavailableResponse(response: Response): boolean {
  return (
    [502, 503, 504].includes(response.status) ||
    (response.status === 403 && response.headers.get("x-mxr-tailnet-required") === "1")
  );
}

const READ_PATHS = new Set([
  "/api/v1/client/shell",
  "/api/v1/platform/accounts",
  "/api/v1/platform/keymap",
  "/api/v1/mail/now",
  "/api/v1/mail/desk",
  "/api/v1/mail/rail",
  "/api/v1/mail/mailbox",
  "/api/v1/mail/people",
  "/api/v1/mail/people/page",
  "/api/v1/mail/modes/guide",
  "/api/v1/mail/modes/membership",
]);

/** Only reads needed to reopen a conversation; never auth, drafts, or actions. */
export function isOfflineRead(url: URL, method: string, origin: string): boolean {
  return (
    method === "GET" &&
    url.origin === origin &&
    (READ_PATHS.has(url.pathname) ||
      /^\/api\/v1\/mail\/threads\/[^/]+(?:\/context(?:\/gist)?|\/modes)?$/.test(url.pathname))
  );
}

export function mayCacheMail(response: Response): boolean {
  return (
    response.status === 200 && !!response.headers.get("content-type")?.includes("application/json")
  );
}

export function clearOfflineMail(): void {
  if (typeof navigator !== "undefined" && "serviceWorker" in navigator) {
    navigator.serviceWorker.controller?.postMessage({ type: "CLEAR_OFFLINE_MAIL" });
  }
}
