/*
 * Plain-language status for an OAuth sign-in session. The daemon reports
 * machine states ("waiting_for_user"); people should read what to do.
 */

import type { AuthSession } from "./api";

export type AuthTone = "pending" | "ready" | "error";

export interface AuthStatus {
  tone: AuthTone;
  text: string;
}

export function isTerminalAuthState(state: string): boolean {
  return state === "authorized" || state === "failed" || state === "cancelled";
}

export function describeAuthSession(
  session: AuthSession | undefined,
  provider: string,
): AuthStatus {
  if (!session) return { tone: "pending", text: `Starting ${provider} sign-in…` };
  switch (session.state) {
    case "starting":
      return { tone: "pending", text: `Starting ${provider} sign-in…` };
    case "waiting_for_user":
      return {
        tone: "pending",
        text: session.user_code
          ? `Enter the code on ${provider}'s page, then come back here.`
          : `Approve access in the ${provider} tab. This page updates on its own.`,
      };
    case "authorized":
      return { tone: "ready", text: "Access approved. Finish to save the account." };
    case "failed":
      return {
        tone: "error",
        text: `Authorization failed${session.error ? `: ${session.error}` : "."} Start over to try again.`,
      };
    case "cancelled":
      return { tone: "error", text: "Authorization was cancelled. Start over to try again." };
    default:
      return { tone: "pending", text: "Waiting for sign-in to finish…" };
  }
}
