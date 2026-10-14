/*
 * Marginalia, the reading app Reading items open in with M. Connecting
 * saves its address in config and the personal token in the daemon's
 * secrets file; the token is checked with Marginalia first and never
 * comes back to this page.
 */

import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useState, type FormEvent } from "react";
import { toast } from "sonner";

import { Field, SettingRow } from "./settingsParts";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import {
  MARGINALIA_KEY,
  setMarginaliaConnection,
  syncMarginaliaHighlights,
  useMarginaliaStatus,
} from "@/features/reading/api";
import { refreshReading } from "@/features/reading/readingVerbs";

function errorText(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

export function MarginaliaSection() {
  const qc = useQueryClient();
  const status = useMarginaliaStatus();
  const [url, setUrl] = useState("");
  const [token, setToken] = useState("");
  const connection = useMutation({
    mutationFn: setMarginaliaConnection,
    onSuccess: (next) => {
      qc.setQueryData(MARGINALIA_KEY, next);
      setToken("");
      toast.success(next.connected ? "Marginalia connected" : "Marginalia disconnected");
    },
    onError: (error) => {
      // The token never stays in the page once it has been sent.
      setToken("");
      toast.error("Couldn't connect Marginalia", { description: errorText(error) });
    },
  });
  const sync = useMutation({
    mutationFn: () => syncMarginaliaHighlights(),
    onSuccess: async (result) => {
      toast.success(
        `Read highlights on ${result.items} item${result.items === 1 ? "" : "s"}: ${result.added} new, ${result.removed} removed.`,
        { description: result.failed?.[0]?.error },
      );
      await refreshReading();
    },
    onError: (error) =>
      toast.error("Couldn't read highlights back", { description: errorText(error) }),
  });

  const connect = (event: FormEvent) => {
    event.preventDefault();
    if (!url.trim() || !token.trim()) return;
    connection.mutate({ url: url.trim(), token: token.trim() });
  };

  const current = status.data;
  return (
    <div>
      <p className="mb-5 max-w-prose text-[13px] text-muted-foreground">
        Press <kbd>M</kbd> on a Reading item to open it in Marginalia, with its themes, margin notes
        and companion. The first time, mxr sends the item's text there; highlights you make in
        Marginalia come back to the item here. Nothing is sent until you press <kbd>M</kbd>.
      </p>
      {current?.connected ? (
        <>
          <SettingRow
            label="Connected"
            description={`${current.url ?? ""} · ${current.sent} item${current.sent === 1 ? "" : "s"} sent`}
          >
            <div className="flex gap-2">
              <Button
                size="sm"
                variant="outline"
                disabled={sync.isPending}
                onClick={() => sync.mutate()}
              >
                {sync.isPending ? "Reading…" : "Read highlights back"}
              </Button>
              <Button
                size="sm"
                variant="outline"
                disabled={connection.isPending}
                onClick={() => connection.mutate(null)}
              >
                Disconnect
              </Button>
            </div>
          </SettingRow>
          <p className="mt-3 text-[12.5px] text-muted-foreground">
            Disconnecting removes the token. Items already sent stay in Marginalia.
          </p>
        </>
      ) : (
        <form className="max-w-md space-y-4" onSubmit={connect}>
          <Field label="Marginalia address" htmlFor="marginalia-url">
            <Input
              id="marginalia-url"
              type="url"
              inputMode="url"
              autoComplete="url"
              placeholder="https://reader.example.com"
              value={url}
              onChange={(event) => setUrl(event.target.value)}
            />
          </Field>
          <Field label="Token" htmlFor="marginalia-token">
            <Input
              id="marginalia-token"
              type="password"
              autoComplete="off"
              spellCheck={false}
              placeholder="mrg_…"
              className="font-mono"
              value={token}
              onChange={(event) => setToken(event.target.value)}
            />
          </Field>
          <p className="text-[12.5px] text-muted-foreground">
            Create a token in Marginalia under Account, Connected apps. mxr checks it with
            Marginalia, then keeps it in its secrets file.
          </p>
          <Button
            type="submit"
            size="sm"
            disabled={connection.isPending || !url.trim() || !token.trim()}
          >
            {connection.isPending ? "Checking…" : "Connect"}
          </Button>
        </form>
      )}
    </div>
  );
}
