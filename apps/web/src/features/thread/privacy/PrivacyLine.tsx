/*
 * One quiet line under a message header saying what the reader blocked and
 * from whom, with the way to load the images. Nothing when nothing was
 * blocked.
 */

import { ShieldCheck } from "lucide-react";
import { useMemo } from "react";

import { KeyChip } from "@/components/KeyChip";

import { analyzeBlockedContent, blockedSentence } from "./blockedContent";

export function PrivacyLine({
  html,
  imagesAllowed,
  onShowImages,
  senderEmail,
  onAlwaysAllowSender,
}: {
  html: string;
  imagesAllowed: boolean;
  onShowImages: () => void;
  senderEmail: string | null;
  onAlwaysAllowSender: (email: string) => void;
}) {
  const content = useMemo(() => analyzeBlockedContent(html), [html]);
  const sentence = blockedSentence(content, imagesAllowed);
  if (!sentence) return null;
  const canShow = !imagesAllowed && content.remoteImages > 0;
  return (
    <p
      data-testid="privacy-line"
      className="mb-3 flex flex-wrap items-center gap-x-3 gap-y-1 text-[12px] leading-5 text-muted-foreground"
    >
      <span className="inline-flex items-center gap-1.5">
        <ShieldCheck className="size-3.5 shrink-0" aria-hidden />
        {sentence}
      </span>
      {canShow ? (
        <button
          type="button"
          onClick={onShowImages}
          className="inline-flex items-center gap-1.5 text-primary hover:underline"
        >
          Show images <KeyChip className="h-4 px-1">M</KeyChip>
        </button>
      ) : null}
      {canShow && senderEmail ? (
        <button
          type="button"
          onClick={() => onAlwaysAllowSender(senderEmail)}
          className="text-muted-foreground underline decoration-border underline-offset-2 hover:text-foreground"
        >
          Always for {senderEmail}
        </button>
      ) : null}
    </p>
  );
}
