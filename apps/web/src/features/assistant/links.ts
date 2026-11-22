import type { components } from "@/api/generated";

export type AssistantLink = components["schemas"]["AssistantLinkData"];

export function linkHref(link: AssistantLink): string {
  if (link.kind === "thread") {
    return `/m/archive/${encodeURIComponent(link.thread_id)}`;
  }
  return `/messages?${new URLSearchParams({ person: link.person_id }).toString()}`;
}

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
