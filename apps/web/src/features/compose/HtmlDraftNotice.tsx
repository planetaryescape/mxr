/*
 * What the browser shows when you open a draft whose body is a supplied HTML
 * document rather than markdown.
 *
 * mxr preserves such a document byte-for-byte on the way to the wire, so the
 * markdown composer cannot represent it and the bridge refuses to open an
 * editing session (`code: "html_draft_not_editable"`). Rather than a dead-end
 * error we show the document read-only, reusing the thread reader's sandboxed
 * iframe — with remote images blocked, so merely looking at your own draft
 * cannot fire a tracking pixel.
 *
 * The sanitising in that iframe is display-only. Nothing here writes back to
 * the stored draft, and there is no compose file to save or send from, so the
 * HTML that eventually reaches the wire is untouched by this preview.
 */

import { FileCode2 } from "lucide-react";

import { MessageBody } from "@/features/thread/MessageBody";
import { useUiPrefs } from "@/state/uiPrefsStore";

/** Bridge error code for "this draft's body is HTML, not markdown". */
export const HTML_DRAFT_NOT_EDITABLE = "html_draft_not_editable";

export interface HtmlDraftRefusal {
  previewHtml: string;
}

/**
 * Recognise the bridge's HTML-draft refusal on a thrown session error, and
 * pull out the document to preview. Returns `null` for every other failure so
 * the caller keeps its normal retry affordance.
 *
 * Shape-checked rather than `instanceof BridgeRequestError` so this component
 * stays independent of the HTTP client. The bridge's own sentence is
 * deliberately not shown: the wording below is the single place this is
 * explained to the user.
 */
export function htmlDraftRefusal(error: unknown): HtmlDraftRefusal | null {
  if (!error || typeof error !== "object") return null;
  const candidate = error as { code?: unknown; details?: unknown };
  if (candidate.code !== HTML_DRAFT_NOT_EDITABLE) return null;
  const details = (candidate.details ?? {}) as { previewHtml?: unknown };
  return {
    previewHtml: typeof details.previewHtml === "string" ? details.previewHtml : "",
  };
}

export function HtmlDraftNotice({ refusal }: { refusal: HtmlDraftRefusal }) {
  const emailHtmlTheme = useUiPrefs((state) => state.emailHtmlTheme);

  return (
    <div className="flex min-w-0 flex-1 flex-col overflow-y-auto bg-background">
      <div className="mx-auto w-full max-w-[860px] space-y-4 px-5 py-6">
        <div className="flex items-start gap-3">
          <FileCode2 className="mt-0.5 size-5 shrink-0 text-muted-foreground" />
          <div className="space-y-1">
            <h2 className="text-sm font-medium">This draft has an HTML body</h2>
            <p className="text-xs text-muted-foreground">
              mxr sends the HTML you supplied exactly as written, so the markdown composer cannot
              edit it without rewriting the document. Edit the source file and re-create the draft
              with <code className="font-mono">mxr compose --html-file &lt;path&gt;</code>.
            </p>
          </div>
        </div>

        <section className="space-y-2">
          <h3 className="text-xs font-medium text-muted-foreground">
            Read-only preview · remote images blocked
          </h3>
          {refusal.previewHtml ? (
            <MessageBody
              html={refusal.previewHtml}
              allowRemoteImages={false}
              theme={emailHtmlTheme}
            />
          ) : (
            // An empty document is a real state worth naming. Rendering nothing
            // here would read as a broken panel rather than an empty draft.
            <p className="rounded-md border border-border px-3 py-2 text-xs text-muted-foreground">
              This draft's HTML body is empty.
            </p>
          )}
        </section>
      </div>
    </div>
  );
}
