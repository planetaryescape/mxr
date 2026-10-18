import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useRef, useState } from "react";

import { Button } from "@/components/ui/button";
import type { components } from "@/api/generated";

import {
  linkMicrosoftTodo,
  syncMicrosoftTodo,
  TODO_KEY,
  useMicrosoftTodoStatus,
  type Todo,
} from "./api";

/** An explicit link: preview the outgoing context before enabling ongoing sync. */
export function MicrosoftTodoAction({ todo }: { todo: Todo }) {
  const status = useMicrosoftTodoStatus();
  const client = useQueryClient();
  const [expanded, setExpanded] = useState(false);
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [syncPreview, setSyncPreview] = useState<
    components["schemas"]["MicrosoftTodoSyncItemData"] | null
  >(null);
  const inFlight = useRef(false);
  const link = useQuery({
    queryKey: ["microsoft-todo", "link", todo.id],
    queryFn: () => linkMicrosoftTodo(todo.id, undefined, todo.account_id),
    enabled: status.data?.installed === true && expanded,
    retry: false,
    staleTime: 0,
  });

  async function commit(previewSync = false) {
    if (inFlight.current || !link.data) return;
    inFlight.current = true;
    setBusy(true);
    setError(null);
    try {
      if (link.data.state === "preview") {
        const result = await linkMicrosoftTodo(todo.id, link.data.fingerprint, todo.account_id);
        client.setQueryData(["microsoft-todo", "link", todo.id], result);
        setMessage(result.summary);
      } else {
        const result = await syncMicrosoftTodo(todo.id, previewSync, todo.account_id);
        setSyncPreview(previewSync ? (result.results[0] ?? null) : null);
        setMessage(result.results.map((item) => item.summary).join(" "));
        if (!previewSync)
          await Promise.all([
            client.invalidateQueries({ queryKey: TODO_KEY }),
            client.invalidateQueries({ queryKey: ["microsoft-todo", "link", todo.id] }),
          ]);
      }
    } catch (cause) {
      setError(
        cause instanceof Error
          ? cause.message
          : "Could not reach Microsoft To Do. Refresh the link status before trying again.",
      );
      // A lost answer is not permission to create again. Read the durable
      // daemon receipt before enabling another attempt.
      await client.invalidateQueries({ queryKey: ["microsoft-todo", "link", todo.id] });
    } finally {
      inFlight.current = false;
      setBusy(false);
    }
  }

  if (status.isError) {
    return (
      <div className="grid gap-2">
        <p role="alert">Could not check Microsoft To Do: {status.error.message}</p>
        <div>
          <Button
            type="button"
            variant="ghost"
            size="sm"
            disabled={status.isFetching}
            onClick={() => void status.refetch()}
          >
            Retry Microsoft To Do status
          </Button>
        </div>
      </div>
    );
  }
  if (!status.data) return null;
  if (!status.data.installed) {
    return (
      <p>
        Microsoft To Do is optional.{" "}
        <a
          className="underline underline-offset-4"
          href={status.data.install_url}
          target="_blank"
          rel="noreferrer"
        >
          Install ms-todo
        </a>{" "}
        on the machine running Mixer to manage linked tasks there.
      </p>
    );
  }
  return (
    <div className="grid gap-2" data-testid="todo-microsoft">
      <div>
        <Button
          type="button"
          variant="ghost"
          size="sm"
          aria-expanded={expanded}
          onClick={() => setExpanded(!expanded)}
        >
          Microsoft To Do
        </Button>
      </div>
      {expanded ? (
        <>
          {link.isPending ? <p role="status">Checking the task link…</p> : null}
          {link.data ? (
            <>
              <p>{link.data.summary}</p>
              {link.data.state === "preview" ? (
                <>
                  <dl className="grid grid-cols-[6.5rem_minmax(0,1fr)] gap-x-3 gap-y-0.5">
                    <dt>Title</dt>
                    <dd className="break-words">{link.data.title}</dd>
                    <dt>Due</dt>
                    <dd>{link.data.due ?? "No due date"}</dd>
                    <dt>Schedule</dt>
                    <dd>{link.data.scheduled ?? "Not scheduled"}</dd>
                  </dl>
                  <p className="whitespace-pre-wrap break-words">{link.data.notes}</p>
                  <p>
                    Scheduling syncs with ms-todo’s defer date, not the phone app’s My Day. Email
                    bodies and attachments are not sent.
                  </p>
                </>
              ) : null}
              {syncPreview ? (
                <dl className="grid gap-2">
                  {(
                    [
                      ["Mixer", syncPreview.mixer],
                      ["Microsoft To Do", syncPreview.microsoft],
                      ["After syncing", syncPreview.merged],
                    ] as const
                  ).map(([label, fields]) =>
                    fields ? (
                      <div key={label}>
                        <dt className="text-text-1">{label}</dt>
                        <dd className="break-words">
                          {fields.title} · due {fields.due ?? "none"} · scheduled{" "}
                          {fields.scheduled ?? "none"} · {fields.completed ? "done" : "open"}
                        </dd>
                      </div>
                    ) : null,
                  )}
                </dl>
              ) : null}
              <div className="flex flex-wrap gap-2">
                {link.data.state !== "unknown" ? (
                  <Button
                    type="button"
                    variant="outline"
                    size="sm"
                    disabled={busy || link.isFetching || link.isError}
                    onClick={() => void commit()}
                  >
                    {busy
                      ? "Working…"
                      : link.data.state === "preview"
                        ? "Link and sync both ways"
                        : "Sync now"}
                  </Button>
                ) : null}
                {link.data.task_id ? (
                  <Button
                    type="button"
                    variant="ghost"
                    size="sm"
                    disabled={busy || link.isFetching}
                    onClick={() => void commit(true)}
                  >
                    Preview sync
                  </Button>
                ) : null}
              </div>
            </>
          ) : null}
          {link.data || link.isError ? (
            <div>
              <Button
                type="button"
                variant="ghost"
                size="sm"
                disabled={busy || link.isFetching}
                onClick={() => void link.refetch()}
              >
                Refresh link status
              </Button>
            </div>
          ) : null}
          {message ? <p role="status">{message}</p> : null}
          {error || link.error ? <p role="alert">{error ?? link.error?.message}</p> : null}
        </>
      ) : null}
    </div>
  );
}
