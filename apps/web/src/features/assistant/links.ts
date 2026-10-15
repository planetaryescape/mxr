/*
 * Where the assistant's links go in the app. Replies link emails and
 * people as `mxr:thread/<id>` and `mxr:person/<id>`; the daemon keeps only
 * links whose id came back in a tool result, and the step readouts carry
 * the same two kinds. Both open in the app, beside the assistant.
 */

import type { components } from "@/api/generated";

export type AssistantLink = components["schemas"]["AssistantLinkData"];

/** The app path a link opens: an email wherever it now is (All Mail), or a person's row in Messages. */
export function linkHref(link: AssistantLink): string {
  if (link.kind === "thread") {
    return `/m/archive/${encodeURIComponent(link.thread_id)}`;
  }
  return `/messages?${new URLSearchParams({ person: link.person_id }).toString()}`;
}

/** An `mxr:` link from a reply, or null for any other href. */
export function parseMxrLink(href: string): AssistantLink | null {
  const match = /^mxr:(thread|person)\/(.+)$/.exec(href);
  if (!match?.[1] || !match[2]) return null;
  let id: string;
  try {
    id = decodeURIComponent(match[2]);
  } catch {
    return null;
  }
  return match[1] === "thread"
    ? { kind: "thread", thread_id: id }
    : { kind: "person", person_id: id };
}
