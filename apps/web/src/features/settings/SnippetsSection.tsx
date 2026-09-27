/*
 * Text snippets, expanded in compose by typing ;name. Stored in the daemon
 * so the CLI and TUI share them.
 */

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Plus } from "lucide-react";
import { useState, type FormEvent } from "react";
import { toast } from "sonner";

import { ConfirmDelete, Field } from "./settingsParts";
import { apiFetch } from "@/api/client";
import { PageError, PageSkeleton, RuledList } from "@/components/PageParts";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";

interface Snippet {
  name: string;
  body: string;
  vars?: string[];
  updated_at?: string;
}

export function SnippetsSection() {
  const qc = useQueryClient();
  const snippets = useQuery({
    queryKey: ["snippets"],
    queryFn: () => apiFetch<{ snippets: Snippet[] }>("/api/v1/mail/snippets"),
  });
  const [name, setName] = useState("");
  const [body, setBody] = useState("");
  const save = useMutation({
    mutationFn: () =>
      apiFetch<unknown>("/api/v1/mail/snippets", {
        method: "POST",
        body: { name: name.trim(), body, vars: [] },
      }),
    onSuccess: () => {
      toast.success(`Saved ;${name.trim()}`);
      setName("");
      setBody("");
      void qc.invalidateQueries({ queryKey: ["snippets"] });
    },
    onError: (error) => toast.error("Snippet save failed", { description: error.message }),
  });
  const remove = useMutation({
    mutationFn: (snippet: string) =>
      apiFetch<unknown>(`/api/v1/mail/snippets/${encodeURIComponent(snippet)}`, {
        method: "DELETE",
      }),
    onSuccess: (_result, snippet) => {
      toast.success(`Deleted ;${snippet}`);
      void qc.invalidateQueries({ queryKey: ["snippets"] });
    },
    onError: (error) => toast.error("Snippet delete failed", { description: error.message }),
  });
  const submit = (event: FormEvent) => {
    event.preventDefault();
    if (name.trim() && body.trim()) save.mutate();
  };
  const rows = snippets.data?.snippets ?? [];

  return (
    <div className="space-y-8">
      <form onSubmit={submit} className="grid max-w-xl gap-3">
        <Field label="Name" htmlFor="snippet-name">
          <Input
            id="snippet-name"
            value={name}
            onChange={(event) => setName(event.target.value)}
            placeholder="thanks"
            className="h-8 font-mono text-xs"
          />
        </Field>
        <Field label="Text" htmlFor="snippet-body">
          <Textarea
            id="snippet-body"
            value={body}
            onChange={(event) => setBody(event.target.value)}
            className="min-h-24 text-[13px]"
            placeholder="Thanks for the quick reply."
          />
        </Field>
        <div>
          <Button type="submit" size="sm" disabled={!name.trim() || !body.trim() || save.isPending}>
            <Plus className="size-3" />
            Save snippet
          </Button>
        </div>
      </form>
      {snippets.isPending ? (
        <PageSkeleton rows={3} label="Loading snippets" />
      ) : snippets.isError ? (
        <PageError
          title="Snippets unavailable"
          error={snippets.error}
          onRetry={() => void snippets.refetch()}
        />
      ) : rows.length === 0 ? (
        <p className="text-[13px] text-muted-foreground">
          No snippets yet. Type ;name in compose to expand one.
        </p>
      ) : (
        <RuledList label="Snippets" className="border-t border-border/60">
          {rows.map((snippet) => (
            <li
              key={snippet.name}
              className="flex items-start justify-between gap-3 border-b border-border/60 py-3"
            >
              <div className="min-w-0">
                <div className="font-mono text-xs text-primary">;{snippet.name}</div>
                <p className="mt-1 whitespace-pre-wrap text-[13px] text-muted-foreground">
                  {snippet.body}
                </p>
              </div>
              <ConfirmDelete
                what={`;${snippet.name}`}
                description="The snippet is removed for the web, CLI and TUI."
                disabled={remove.isPending}
                onConfirm={() => remove.mutate(snippet.name)}
              />
            </li>
          ))}
        </RuledList>
      )}
    </div>
  );
}
