import { Action, ActionPanel, Color, Icon, List } from "@raycast/api";
import { useCachedPromise } from "@raycast/utils";
import { useState } from "react";
import { getUpdates, letGoDigest, markDone } from "./bridge/modes";
import type { UpdateLine, UpdatesDigest } from "./bridge/types";
import { AccountDropdown } from "./components/AccountDropdown";
import { useAccountBadge } from "./components/accountBadge";
import { useLiveRefresh } from "./components/live";
import { SearchSection, useMailSearch } from "./components/MailRow";
import { senderFace } from "./components/faces";
import { MailPane, PaneAction, searchRowThread, usePane, usePreview } from "./components/pane";
import { ReadAction } from "./components/ThreadDetail";
import { itemErrors, runPreviewed } from "./components/previewed";
import { arrival } from "./components/time";
import { useWebBase, web } from "./components/web";
import { keyFor } from "./components/keys";

export default function Updates() {
  const [account, setAccount] = useState<string>();
  const { data, isLoading, revalidate } = useCachedPromise(getUpdates, [account], {
    keepPreviousData: true,
  });
  const webBase = useWebBase();
  const search = useMailSearch(account);
  const shown = (lines: UpdateLine[] | undefined) =>
    (lines ?? []).filter((line) => search.keep(line.source_name, line.fact));

  const pane = usePane("updates");
  const lines = [...(data?.needs_a_look ?? []), ...(data?.changed ?? []), ...(data?.routine ?? [])];
  const selectedThread =
    searchRowThread(pane.selected) ??
    lines.find((line) => line.id === pane.selected)?.latest_thread_id ??
    undefined;
  const preview = usePreview(pane.showing ? selectedThread : undefined);

  const letGo = (digest: UpdatesDigest) =>
    runPreviewed({
      title: "Let go of this digest?",
      confirm: "Let Go",
      preview: () => letGoDigest(digest, true),
      commit: () => letGoDigest(digest, false),
      describe: (reply) => reply.result.line,
      failures: (reply) => itemErrors(reply.result.items),
      undoId: (reply) => reply.result.mutation_id,
      done: "Let go",
      refresh: revalidate,
    });

  const item = (line: UpdateLine, attention: boolean) => (
    <List.Item
      key={line.id}
      id={line.id}
      icon={senderFace(line.source_name, line.sender_email)}
      title={line.source_name}
      subtitle={pane.showing ? undefined : line.fact}
      accessories={
        pane.showing
          ? undefined
          : [
              ...(attention
                ? [
                    {
                      icon: { source: Icon.ExclamationMark, tintColor: Color.Orange },
                      tooltip: line.todo_suggestion ?? "Needs a look",
                    },
                  ]
                : []),
              ...(line.code
                ? [{ tag: { value: line.code, color: Color.Green }, tooltip: "One-time code" }]
                : []),
              ...badge(line.account_id),
              ...(line.count > 1 ? [{ text: `${line.count}` }] : []),
              arrival(line.latest_at),
            ]
      }
      detail={
        pane.showing ? (
          <MailPane
            subject={line.fact}
            fromName={line.source_name}
            fromEmail={line.sender_email}
            date={line.latest_at}
            account={badge.tagOf(line.account_id)}
            preview={pane.selected === line.id ? preview : undefined}
            fallback={line.why}
          >
            {line.code ? (
              <List.Item.Detail.Metadata.Label
                title="One-Time Code"
                text={{ value: line.code, color: Color.Green }}
                icon={Icon.Key}
              />
            ) : null}
            {line.todo_suggestion ? (
              <List.Item.Detail.Metadata.Label title="Suggested" text={line.todo_suggestion} />
            ) : null}
            {line.count > 1 ? (
              <List.Item.Detail.Metadata.Label title="Emails" text={String(line.count)} />
            ) : null}
          </MailPane>
        ) : undefined
      }
      actions={
        <ActionPanel>
          <ActionPanel.Section>
            {line.code && (
              <Action.CopyToClipboard
                title="Copy Code"
                icon={Icon.Key}
                content={line.code}
                concealed
              />
            )}
            {line.latest_thread_id && (
              <ReadAction
                threadId={line.latest_thread_id}
                webPath={web.thread("updates", line.latest_thread_id)}
                onChange={revalidate}
              />
            )}
          </ActionPanel.Section>
          <ActionPanel.Section>
            {line.thread_ids && line.thread_ids.length > 0 && (
              <Action
                title="Done Here"
                icon={Icon.Checkmark}
                shortcut={keyFor("updates.let-go-source")}
                onAction={() => {
                  const threads = line.thread_ids ?? [];
                  return runPreviewed({
                    title: `Done with ${line.source_name}?`,
                    confirm: "Done Here",
                    preview: () => markDone("updates", threads, true),
                    commit: () => markDone("updates", threads, false),
                    describe: (done) => done.items.map((outcome) => outcome.copy).join("\n"),
                    failures: (done) => itemErrors(done.items),
                    undoId: (done) => done.mutation_id,
                    done: "Done here",
                    refresh: revalidate,
                  });
                }}
              />
            )}
            {data && (
              <Action
                title="Let Go of This Digest"
                icon={Icon.Tray}
                shortcut={keyFor("updates.let-go-all")}
                onAction={() => letGo(data)}
              />
            )}
          </ActionPanel.Section>
          <ActionPanel.Section>
            {webBase && (
              <Action.OpenInBrowser
                title="Open in mxr"
                url={`${webBase}${line.latest_thread_id ? web.thread("updates", line.latest_thread_id) : web.mode("updates")}`}
                shortcut={keyFor("updates.open-email")}
              />
            )}
            {line.link?.url && <Action.OpenInBrowser title="Open Link" url={line.link.url} />}
            <PaneAction pane={pane} />
          </ActionPanel.Section>
        </ActionPanel>
      }
    />
  );
  const badge = useAccountBadge(account);
  useLiveRefresh(revalidate);

  return (
    <List
      isLoading={isLoading || search.isLoading}
      isShowingDetail={pane.showing && (lines.length > 0 || search.results.length > 0)}
      onSelectionChange={pane.select}
      navigationTitle={data?.header ?? "Updates"}
      searchBarPlaceholder="Filter updates and search all mail"
      searchBarAccessory={<AccountDropdown onChange={setAccount} />}
      {...search.listProps}
    >
      {data?.empty_state && !search.searching && (
        <List.EmptyView icon={Icon.Bell} title={data.empty_state} />
      )}
      <List.Section title="Needs a Look" subtitle={data?.headline}>
        {shown(data?.needs_a_look).map((line) => item(line, true))}
      </List.Section>
      <List.Section title="Changed">
        {shown(data?.changed).map((line) => item(line, false))}
      </List.Section>
      <List.Section title="Routine">
        {shown(data?.routine).map((line) => item(line, false))}
      </List.Section>
      <SearchSection
        search={search}
        badge={badge}
        pane={pane}
        preview={preview}
        onChange={revalidate}
      />
    </List>
  );
}
