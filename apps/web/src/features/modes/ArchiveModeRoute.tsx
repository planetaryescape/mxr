import { Link } from "@tanstack/react-router";
import { FolderArchive } from "lucide-react";

import { KeyChip } from "@/components/KeyChip";

import { EarlyModeNote } from "./EarlyModeNote";

/**
 * Archive, honestly: records (receipts, orders, bookings, documents) are
 * not built yet, so this says what Archive will be and sends a search to
 * the search page, where receipts and records already turn up. No fake
 * cards.
 */
export function ArchiveModeRoute() {
  return (
    <section aria-label="Archive" className="flex min-h-0 min-w-0 flex-1 flex-col bg-background">
      <header className="shrink-0 border-b border-border px-5 pb-3 pt-4">
        <h1 className="text-[17px] font-semibold tracking-tight text-foreground">Archive</h1>
      </header>
      <EarlyModeNote mode="archive" />
      <div className="flex flex-1 flex-col items-center justify-center gap-2 px-8 py-16 text-center">
        <FolderArchive aria-hidden className="size-6 text-muted-foreground" />
        <p className="text-[15px] text-foreground/90">Records are coming.</p>
        <p className="max-w-md text-pretty text-[13px] text-muted-foreground">
          Archive will keep one card per order, trip or bill, built from your mail, and answer with
          the field you ask for. Until then, search finds receipts and bookings in all your mail.
        </p>
        <p className="mt-2 flex items-center gap-2 text-[13px]">
          <Link
            to="/search"
            className="underline decoration-border-strong underline-offset-4 hover:decoration-primary"
          >
            Search all mail
          </Link>
          <KeyChip>/</KeyChip>
        </p>
      </div>
    </section>
  );
}
