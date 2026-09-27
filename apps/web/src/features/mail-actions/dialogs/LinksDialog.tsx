import { useQuery } from "@tanstack/react-query";
import { Copy, ExternalLink, Link as LinkIcon } from "lucide-react";
import { find as findLinks } from "linkifyjs";
import { useEffect, useMemo, useRef, useState } from "react";
import { toast } from "sonner";

import { KeyChip } from "@/components/KeyChip";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { fetchThread } from "@/features/mailbox/api";
import type { MessageBodyView } from "@/features/mailbox/types";
import { cn } from "@/lib/utils";

import { describeTarget } from "../mailVerbs";
import type { MailTarget } from "../target";

export interface ExtractedLink {
  href: string;
  text: string;
}

const SAFE_PROTOCOLS = new Set(["http:", "https:", "mailto:"]);

/** Every distinct http(s)/mailto link in the bodies, HTML anchors first. */
export function extractLinks(bodies: MessageBodyView[]): ExtractedLink[] {
  const seen = new Map<string, ExtractedLink>();
  const add = (href: string, text: string) => {
    let url: URL;
    try {
      url = new URL(href);
    } catch {
      return;
    }
    if (!SAFE_PROTOCOLS.has(url.protocol)) return;
    const key = url.toString();
    const label = text.replace(/\s+/g, " ").trim();
    const existing = seen.get(key);
    if (!existing || (!existing.text && label)) seen.set(key, { href: key, text: label });
  };
  for (const body of bodies) {
    if (body.text_html) {
      const doc = new DOMParser().parseFromString(body.text_html, "text/html");
      for (const anchor of Array.from(doc.querySelectorAll("a[href]"))) {
        add(anchor.getAttribute("href") ?? "", anchor.textContent ?? "");
      }
    }
    const text = body.reader_text ?? body.text_plain ?? "";
    for (const match of findLinks(text, { defaultProtocol: "https" })) add(match.href, "");
  }
  return [...seen.values()];
}

function openLink(link: ExtractedLink | undefined) {
  if (!link) return;
  window.open(link.href, "_blank", "noopener,noreferrer");
}

export function LinksDialog({ target, onClose }: { target: MailTarget; onClose: () => void }) {
  const thread = useQuery({
    queryKey: ["thread", target.threadId],
    // Same key the reader uses, so an open thread is already cached.
    queryFn: () => fetchThread(target.threadId ?? ""),
    enabled: Boolean(target.threadId),
  });
  const links = useMemo(() => extractLinks(thread.data?.bodies ?? []), [thread.data]);
  const [index, setIndex] = useState(0);
  const listRef = useRef<HTMLUListElement>(null);

  useEffect(() => {
    listRef.current
      ?.querySelector<HTMLElement>(`[data-index="${index}"]`)
      ?.scrollIntoView({ block: "nearest" });
  }, [index]);

  const copy = (link: ExtractedLink | undefined) => {
    if (!link) return;
    void navigator.clipboard
      .writeText(link.href)
      .then(() => toast.success("Link copied"))
      .catch(() => toast.error("Couldn't copy to the clipboard"));
  };

  return (
    <Dialog open onOpenChange={(next) => !next && onClose()}>
      <DialogContent
        className="max-w-2xl"
        onKeyDown={(event) => {
          if (event.key === "j" || event.key === "ArrowDown") {
            event.preventDefault();
            setIndex((value) => Math.min(links.length - 1, value + 1));
          } else if (event.key === "k" || event.key === "ArrowUp") {
            event.preventDefault();
            setIndex((value) => Math.max(0, value - 1));
          } else if (event.key === "Enter" || event.key === "o") {
            event.preventDefault();
            openLink(links[index]);
          } else if (event.key === "y") {
            event.preventDefault();
            copy(links[index]);
          }
        }}
      >
        <DialogHeader>
          <DialogTitle>Links</DialogTitle>
          <DialogDescription className="truncate">{describeTarget(target)}</DialogDescription>
        </DialogHeader>
        {thread.isLoading ? (
          <div className="grid gap-1">
            {Array.from({ length: 4 }, (_, i) => (
              <div key={i} className="h-10 animate-pulse rounded bg-muted/60" />
            ))}
          </div>
        ) : links.length === 0 ? (
          <p className="py-6 text-center text-sm text-muted-foreground">
            This conversation has no links.
          </p>
        ) : (
          <ul
            ref={listRef}
            role="listbox"
            aria-label="Links"
            tabIndex={0}
            className="max-h-[55vh] overflow-auto outline-none"
          >
            {links.map((link, i) => (
              <li
                key={link.href}
                data-index={i}
                role="option"
                aria-selected={i === index}
                onMouseEnter={() => setIndex(i)}
                onClick={() => openLink(link)}
                className={cn(
                  "group flex cursor-pointer items-center gap-3 rounded-md px-3 py-2",
                  i === index ? "bg-accent" : "hover:bg-accent/60",
                )}
              >
                <LinkIcon className="size-3.5 shrink-0 text-muted-foreground" />
                <span className="min-w-0 flex-1">
                  {link.text ? (
                    <span className="block truncate text-[13px]">{link.text}</span>
                  ) : null}
                  <span className="block truncate font-mono text-2xs text-muted-foreground">
                    {link.href}
                  </span>
                </span>
                <button
                  type="button"
                  tabIndex={-1}
                  aria-label="Copy link"
                  onClick={(event) => {
                    event.stopPropagation();
                    copy(link);
                  }}
                  className="invisible rounded p-1 text-muted-foreground hover:text-foreground group-hover:visible"
                >
                  <Copy className="size-3.5" />
                </button>
                <ExternalLink className="size-3.5 text-muted-foreground" />
              </li>
            ))}
          </ul>
        )}
        <p className="flex items-center gap-2 text-2xs text-muted-foreground">
          <KeyChip>j</KeyChip>
          <KeyChip>k</KeyChip> move <KeyChip>Enter</KeyChip> open <KeyChip>y</KeyChip> copy
        </p>
      </DialogContent>
    </Dialog>
  );
}
