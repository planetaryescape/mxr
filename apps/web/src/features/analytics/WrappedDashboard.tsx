import { useQuery } from "@tanstack/react-query";
import { ChevronLeft, ChevronRight, Clipboard, Sparkles } from "lucide-react";
import { useState, type ReactNode } from "react";
import { toast } from "sonner";

import { fetchWrapped, type AnalyticsRange, type WrappedSummary } from "./api";
import { BarList, Figure, formatDuration, useDrillToSearch, useOpenThread } from "./analyticsParts";
import { PageSection } from "@/components/Page";
import {
  FactList,
  PageEmpty,
  PageError,
  PageSkeleton,
  RuledList,
  RuledRow,
} from "@/components/PageParts";
import { Button } from "@/components/ui/button";
import { plural } from "@/lib/format";
import { formatBytes } from "@/lib/utils";

const DAYS = ["Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday"];

export function WrappedDashboard({ range }: { range: AnalyticsRange }) {
  const wrapped = useQuery({
    queryKey: ["analytics", "wrapped", range],
    queryFn: () => fetchWrapped(range),
  });
  const [story, setStory] = useState(false);

  if (wrapped.isPending) return <PageSkeleton rows={8} label="Loading wrapped" />;
  if (wrapped.isError)
    return (
      <PageError
        title="Wrapped unavailable"
        error={wrapped.error}
        onRetry={() => void wrapped.refetch()}
      />
    );
  const summary = wrapped.data.summary;
  const total = summary.volume.inbound_count + summary.volume.outbound_count;
  if (total === 0)
    return (
      <PageEmpty
        icon={<Sparkles className="size-5" />}
        title="No mail in this window"
        body="Pick a longer range to build a Wrapped."
      />
    );

  return (
    <div>
      <div className="mb-5 flex flex-wrap items-center justify-end gap-2">
        <Button size="sm" variant="outline" onClick={() => void copySummary(summary)}>
          <Clipboard className="size-3" />
          Copy summary
        </Button>
        <Button
          size="sm"
          variant={story ? "secondary" : "outline"}
          aria-pressed={story}
          onClick={() => setStory((value) => !value)}
        >
          {story ? "Show everything" : "Story mode"}
        </Button>
      </div>
      {story ? <WrappedStory summary={summary} /> : <WrappedSections summary={summary} />}
    </div>
  );
}

function WrappedSections({ summary }: { summary: WrappedSummary }) {
  const drill = useDrillToSearch();
  const openThread = useOpenThread();
  const {
    volume,
    time_patterns: when,
    top_contacts: contacts,
    storage,
    newsletters,
    superlatives,
  } = summary;
  const reply = summary.reply_discipline;
  return (
    <div className="gap-x-10 lg:columns-2 [&>section]:break-inside-avoid">
      <PageSection title="Volume" description={windowLabel(summary)}>
        <Figure
          label="Messages"
          value={(volume.inbound_count + volume.outbound_count).toLocaleString()}
          caption={`${volume.inbound_count.toLocaleString()} received, ${volume.outbound_count.toLocaleString()} sent, across ${plural(volume.thread_count, "thread")}`}
        />
      </PageSection>
      <PageSection title="When">
        <FactList
          columns={1}
          facts={[
            ["Busiest day", when.busiest_day_of_week ?? "n/a"],
            [
              "Busiest hour (UTC)",
              when.busiest_hour_utc == null
                ? "n/a"
                : `${String(when.busiest_hour_utc).padStart(2, "0")}:00`,
            ],
            [
              "Busiest date",
              when.busiest_date
                ? `${new Date(when.busiest_date).toLocaleDateString(undefined, { month: "short", day: "numeric", year: "numeric" })} (${plural(when.busiest_date_count, "message")})`
                : "n/a",
            ],
          ]}
        />
        <BarList
          className="mt-4"
          label="Messages by day of week"
          rows={(when.day_of_week_distribution ?? []).map((count, index) => ({
            id: DAYS[index] ?? String(index),
            label: DAYS[index] ?? String(index),
            value: count,
            display: count.toLocaleString(),
          }))}
        />
      </PageSection>
      <PageSection title="People who wrote most">
        {contacts.most_emailed_to_me.length === 0 ? (
          <p className="text-[13px] text-muted-foreground">Nobody yet.</p>
        ) : (
          <RuledList label="Top senders">
            {contacts.most_emailed_to_me.map((contact) => (
              <RuledRow
                key={contact.email}
                title={contact.display_name || contact.email}
                meta={contact.email}
                aside={plural(contact.count, "message")}
                onOpen={() => drill(`from:${contact.email}`)}
                openLabel={`Search mail from ${contact.email}`}
              />
            ))}
          </RuledList>
        )}
      </PageSection>
      <PageSection title="Replies">
        {reply ? (
          <FactList
            columns={1}
            facts={[
              ["Median reply", formatDuration(reply.clock_p50_seconds)],
              ["90th percentile", formatDuration(reply.clock_p90_seconds)],
              [
                "Fastest",
                reply.fastest
                  ? `${formatDuration(reply.fastest.latency_seconds)} to ${reply.fastest.counterparty_email}`
                  : "n/a",
              ],
              [
                "Slowest",
                reply.slowest
                  ? `${formatDuration(reply.slowest.latency_seconds)} to ${reply.slowest.counterparty_email}`
                  : "n/a",
              ],
            ]}
          />
        ) : (
          <p className="text-[13px] text-muted-foreground">No replies sent in this window.</p>
        )}
      </PageSection>
      <PageSection title="Storage and newsletters">
        <FactList
          columns={1}
          facts={[
            ["Stored", formatBytes(storage.total_bytes)],
            [
              "Heaviest type",
              storage.top_mimetype
                ? `${storage.top_mimetype.key} (${formatBytes(storage.top_mimetype.bytes)})`
                : "n/a",
            ],
            ["Mailing lists", newsletters.unique_lists.toLocaleString()],
            ["Share of inbound", `${Math.round(newsletters.list_share_of_inbound_pct)}%`],
          ]}
        />
        {storage.heaviest_message ? (
          <RuledList label="Heaviest message" className="mt-4">
            <RuledRow
              title={storage.heaviest_message.subject || "(no subject)"}
              meta={`Heaviest message · ${storage.heaviest_message.from_email}`}
              aside={formatBytes(storage.heaviest_message.size_bytes)}
              onOpen={() => drill(`from:${storage.heaviest_message?.from_email ?? ""}`)}
            />
          </RuledList>
        ) : null}
      </PageSection>
      <PageSection title="Superlatives">
        <RuledList label="Superlatives">
          {superlatives.longest_thread ? (
            <RuledRow
              title={superlatives.longest_thread.subject || "(no subject)"}
              meta={`Longest thread · ${plural(superlatives.longest_thread.message_count, "message")}`}
              onOpen={() => openThread(superlatives.longest_thread?.thread_id ?? "")}
            />
          ) : null}
          {superlatives.most_ghosted ? (
            <RuledRow
              title={superlatives.most_ghosted.email}
              meta={`Most ghosted · ${superlatives.most_ghosted.inbound_count} in, ${superlatives.most_ghosted.outbound_count} out`}
              onOpen={() => drill(`from:${superlatives.most_ghosted?.email ?? ""}`)}
            />
          ) : null}
        </RuledList>
      </PageSection>
    </div>
  );
}

/** One fact per screen; arrow keys or the buttons move between them. */
function WrappedStory({ summary }: { summary: WrappedSummary }) {
  const tiles = storyTiles(summary);
  const [index, setIndex] = useState(0);
  const tile = tiles[Math.min(index, tiles.length - 1)];
  const move = (delta: number) =>
    setIndex((current) => Math.max(0, Math.min(tiles.length - 1, current + delta)));
  if (!tile) return null;
  return (
    <section
      data-testid="wrapped-story"
      aria-roledescription="slide show"
      aria-label="Wrapped story"
      tabIndex={0}
      // Local arrow keys while the story has focus; preventDefault keeps
      // the app dispatcher from also treating them as list motion.
      onKeyDown={(event) => {
        if (event.key === "ArrowRight" || event.key === "ArrowDown") {
          event.preventDefault();
          move(1);
        } else if (event.key === "ArrowLeft" || event.key === "ArrowUp") {
          event.preventDefault();
          move(-1);
        }
      }}
      className="border-y border-border px-6 py-16 text-center outline-none focus-visible:ring-2 focus-visible:ring-ring"
    >
      <div
        className="font-mono text-2xs uppercase tracking-[0.12em] text-muted-foreground"
        aria-live="polite"
      >
        {index + 1} of {tiles.length}
      </div>
      <h3 className="mt-8 font-mono text-[10.5px] uppercase tracking-[0.12em] text-muted-foreground">
        {tile.title}
      </h3>
      <div className="mt-2 text-5xl font-semibold tracking-tight">{tile.value}</div>
      <div className="mt-3 text-[13px] text-muted-foreground">{tile.meta}</div>
      <div className="mt-10 flex justify-center gap-2">
        <Button
          size="sm"
          variant="outline"
          disabled={index === 0}
          onClick={() => move(-1)}
          aria-label="Previous"
        >
          <ChevronLeft className="size-3.5" />
        </Button>
        <Button
          size="sm"
          variant="outline"
          disabled={index >= tiles.length - 1}
          onClick={() => move(1)}
          aria-label="Next"
        >
          <ChevronRight className="size-3.5" />
        </Button>
      </div>
    </section>
  );
}

interface StoryTile {
  title: string;
  value: ReactNode;
  meta: string;
}

function storyTiles(summary: WrappedSummary): StoryTile[] {
  const { volume, time_patterns: when, top_contacts: contacts, storage, superlatives } = summary;
  const tiles: StoryTile[] = [
    {
      title: "Messages",
      value: (volume.inbound_count + volume.outbound_count).toLocaleString(),
      meta: `${volume.inbound_count.toLocaleString()} received, ${volume.outbound_count.toLocaleString()} sent`,
    },
  ];
  if (when.busiest_day_of_week)
    tiles.push({
      title: "Busiest day",
      value: when.busiest_day_of_week,
      meta: plural(when.busiest_day_of_week_count, "message"),
    });
  const top = contacts.most_emailed_to_me[0];
  if (top)
    tiles.push({
      title: "Wrote to you most",
      value: top.display_name || top.email,
      meta: plural(top.count, "message"),
    });
  if (summary.reply_discipline)
    tiles.push({
      title: "Median reply",
      value: formatDuration(summary.reply_discipline.clock_p50_seconds),
      meta: plural(summary.reply_discipline.sample_count, "reply", "replies"),
    });
  tiles.push({
    title: "Stored",
    value: formatBytes(storage.total_bytes),
    meta: storage.top_mimetype ? `mostly ${storage.top_mimetype.key}` : "",
  });
  if (superlatives.longest_thread)
    tiles.push({
      title: "Longest thread",
      value: plural(superlatives.longest_thread.message_count, "message"),
      meta: superlatives.longest_thread.subject,
    });
  return tiles;
}

function shortDate(value: string): string {
  return new Date(value).toLocaleDateString(undefined, {
    month: "short",
    day: "numeric",
    year: "numeric",
  });
}

function windowLabel(summary: WrappedSummary): string {
  return `${shortDate(summary.window_start)} to ${shortDate(summary.window_end)}`;
}

/**
 * Plain-text summary for pasting into chat or notes. The old button
 * claimed "Share as image" but copied text; an honest text copy is the
 * useful thing here, and it avoids a second canvas renderer to maintain.
 */
export function wrappedSummaryText(summary: WrappedSummary): string {
  const lines = [`mxr wrapped: ${windowLabel(summary)}`];
  for (const tile of storyTiles(summary)) {
    lines.push(
      `${tile.title}: ${typeof tile.value === "string" ? tile.value : String(tile.value)}${tile.meta ? ` (${tile.meta})` : ""}`,
    );
  }
  return lines.join("\n");
}

async function copySummary(summary: WrappedSummary) {
  try {
    await navigator.clipboard.writeText(wrappedSummaryText(summary));
    toast.success("Wrapped summary copied");
  } catch (error) {
    toast.error("Copy failed", {
      description: error instanceof Error ? error.message : String(error),
    });
  }
}
