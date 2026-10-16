import { BridgeRequestError, DaemonUnavailableError, UnauthorizedError } from "@/api/client";

/**
 * The one fix a failure offers. "retry" appears only for failures a second
 * attempt can change; a lookup that can't succeed (a bad id) offers the way
 * out instead (docs/blueprint/27 "States").
 */
export type ErrorFix = "retry" | "inbox" | "start" | "update" | "token" | "none";

export interface ErrorExplanation {
  /** What happened, in plain words. */
  heading: string;
  /** What still works. */
  body: string;
  fix: ErrorFix;
  /** True for the safe sentence given to an error nobody mapped; a caller may name what failed instead. */
  generic: boolean;
  /** The raw error, for the "Details" disclosure. */
  technical: string;
}

const NO_DETAIL = "No additional detail was provided.";

/**
 * Turn what the bridge returned into sentences. `subject` names what was being
 * asked for ("this conversation"); giving one marks a lookup, whose bad or
 * unknown id offers the way out instead of a retry. A page's own name goes in
 * `PageError`'s `title`. Unknown errors get the generic sentence, and their raw
 * text goes under Details.
 */
export function explainError(error: unknown, subject?: string): ErrorExplanation {
  const technical = error instanceof Error && error.message ? error.message : NO_DETAIL;
  const generic = (): ErrorExplanation => ({
    heading: subject === undefined ? "Something went wrong" : `Couldn't load ${subject}`,
    body: "What's already on screen still works. Trying again often clears it.",
    fix: "retry",
    generic: true,
    technical,
  });

  if (error instanceof DaemonUnavailableError) {
    return {
      heading: "mxr isn't running",
      body: "Nothing from this page was loaded before it stopped. It fills in when mxr is back.",
      fix: "start",
      generic: false,
      technical,
    };
  }
  if (error instanceof UnauthorizedError) {
    return {
      heading: "mxr doesn't recognise this browser",
      body: "Mail already on screen is still readable. A fresh bridge token reconnects it.",
      fix: "token",
      generic: false,
      technical,
    };
  }
  if (!(error instanceof BridgeRequestError)) return generic();

  const { status, code } = error;
  if (code === "protocol_mismatch" || status === 426) {
    return {
      heading: "mxr needs updating",
      body: "This web app and the running mxr speak different versions. Mail already loaded is still readable.",
      fix: "update",
      generic: false,
      technical,
    };
  }
  const lookup = subject !== undefined;
  if (
    status === 404 ||
    code === "not_found" ||
    (lookup && (status === 400 || code === "bad_request"))
  ) {
    return {
      heading: lookup ? `Couldn't find ${subject}` : "That isn't here",
      body: "It may have been deleted, or the link is out of date. Your mail is untouched.",
      fix: "inbox",
      generic: false,
      technical,
    };
  }
  if (status === 400 || code === "bad_request") {
    // Asking again would send the same request.
    return {
      ...generic(),
      body: "mxr couldn't use that request. Nothing was changed, and everything else works.",
      fix: "none",
    };
  }
  if (status === 408 || status === 504 || code === "timeout") {
    return {
      heading: "mxr took too long to answer",
      body: "It may still be working. Everything already loaded is still here.",
      fix: "retry",
      generic: false,
      technical,
    };
  }
  if (status === 401) return explainError(new UnauthorizedError(), subject);
  if (status === 403 || code === "policy") {
    return {
      heading: "mxr isn't allowed to do that",
      body: "Nothing was changed. Everything else works as before.",
      fix: "none",
      generic: false,
      technical,
    };
  }
  if (status === 429 || code === "rate_limited") {
    return {
      heading: "The mail provider asked mxr to slow down",
      body: "What's already loaded is still here. Try again in a minute.",
      fix: "retry",
      generic: false,
      technical,
    };
  }
  if (code === "unsupported" || status === 422) {
    return {
      heading: "mxr can't do that here",
      body: "Nothing was changed. Everything else works as before.",
      fix: "none",
      generic: false,
      technical,
    };
  }
  return generic();
}
