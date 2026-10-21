import { Action, ActionPanel, Color, Icon, List, Keyboard } from "@raycast/api";
import { useCachedPromise } from "@raycast/utils";
import { getUpdates, letGoDigest, markDone } from "./bridge/modes";
import type { UpdateLine, UpdatesDigest } from "./bridge/types";
import { runPreviewed } from "./components/previewed";
import { useWebBase, web } from "./components/web";

export default function Updates() {
  const { data, isLoading, revalidate } = useCachedPromise(getUpdates, [], {
    keepPreviousData: true,
  });
  const webBase = useWebBase();

  const letGo = (digest: UpdatesDigest) =>
    runPreviewed({
      title: "Let go of this digest?",
      confirm: "Let Go",
      preview: () => letGoDigest(digest, true),
      commit: () => letGoDigest(digest, false),
      describe: (reply) => reply.result.line,
      undoId: (reply) => reply.result.mutation_id,
      done: "Let go",
      refresh: revalidate,
    });

  const item = (line: UpdateLine, attention: boolean) => (
    <List.Item
      key={line.id}
      icon={attention ? { source: Icon.ExclamationMark, tintColor: Color.Orange } : Icon.Bell}
      title={line.source_name}
      subtitle={line.fact}
      accessories={[
        ...(line.count > 1 ? [{ text: `${line.count}` }] : []),
        { date: new Date(line.latest_at) },
      ]}
      actions={
        <ActionPanel>
          {webBase && (
            <Action.OpenInBrowser
              title="Open in mxr"
              url={`${webBase}${line.latest_thread_id ? web.thread("updates", line.latest_thread_id) : web.mode("updates")}`}
              shortcut={Keyboard.Shortcut.Common.Open}
            />
          )}
          {line.link?.url && <Action.OpenInBrowser title="Open Link" url={line.link.url} />}
          {line.thread_ids && line.thread_ids.length > 0 && (
            <Action
              title="Done Here"
              icon={Icon.Checkmark}
              shortcut={Keyboard.Shortcut.Common.Edit}
              onAction={() => {
                const threads = line.thread_ids ?? [];
                return runPreviewed({
                  title: `Done with ${line.source_name}?`,
                  confirm: "Done Here",
                  preview: () => markDone("updates", threads, true),
                  commit: () => markDone("updates", threads, false),
                  describe: (done) => done.items.map((outcome) => outcome.copy).join("\n"),
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
              shortcut={{ modifiers: ["cmd", "shift"], key: "e" }}
              onAction={() => letGo(data)}
            />
          )}
        </ActionPanel>
      }
    />
  );

  return (
    <List
      isLoading={isLoading}
      navigationTitle={data?.header ?? "Updates"}
      searchBarPlaceholder="Filter updates"
    >
      {data?.empty_state && <List.EmptyView icon={Icon.Bell} title={data.empty_state} />}
      <List.Section title="Needs a Look" subtitle={data?.headline}>
        {data?.needs_a_look.map((line) => item(line, true))}
      </List.Section>
      <List.Section title="Changed">{data?.changed.map((line) => item(line, false))}</List.Section>
      <List.Section title="Routine">{data?.routine.map((line) => item(line, false))}</List.Section>
    </List>
  );
}
