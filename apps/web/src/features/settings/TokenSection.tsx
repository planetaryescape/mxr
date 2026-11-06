import { Eye, EyeOff } from "lucide-react";
import { useState } from "react";

import { Alert } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { useBridgeToken } from "@/hooks/useBridgeToken";
import { getBridgeBaseUrl, getToken } from "@/lib/tokenStorage";
import { assertTarget } from "@/lib/workspaceOwner";

export function TokenSection() {
  const { token, setToken, clearToken, hasToken } = useBridgeToken();
  const [draft, setDraft] = useState(token);
  const [reveal, setReveal] = useState(false);
  const reason = new URLSearchParams(window.location.search).get("reason");
  const [error, setError] = useState<string | null>(
    reason === "expired"
      ? "The bridge token was rejected. Paste a valid token to reconnect."
      : null,
  );
  const [checking, setChecking] = useState(false);

  return (
    <form
      action="/now"
      method="get"
      aria-busy={checking}
      onSubmit={async (event) => {
        event.preventDefault();
        if (checking || !draft.trim()) return;
        const form = event.currentTarget;
        const candidate = draft.trim();
        const target = getBridgeBaseUrl();
        setChecking(true);
        setError(null);
        let saving = false;
        try {
          const response = await fetch(`${target}/api/v1/admin/status`, {
            headers: { Authorization: `Bearer ${candidate}` },
            signal: AbortSignal.timeout(10_000),
          });
          assertTarget(target);
          if (response.status === 401) {
            setError("The bridge token was rejected. Paste a valid token to reconnect.");
          } else if (!response.ok) {
            setError(`The mail bridge returned ${response.status}. Try connecting again.`);
          } else {
            saving = true;
            setToken(candidate);
            if (getToken() !== candidate) throw new Error("Browser storage unavailable");
            // A fresh page drops failed queries and the stale expired-token URL.
            form.submit();
          }
        } catch {
          setError(
            saving
              ? "Couldn't save the token in this browser. Allow site data for mxr and try again."
              : "Couldn't reach the mail bridge. Check your connection and try again.",
          );
        } finally {
          setChecking(false);
        }
      }}
    >
      <p className="mb-5 max-w-prose text-sm text-muted-foreground">
        Paste the bridge token from your active mxr profile to connect to your mail. Most launches
        on the same computer connect automatically.
      </p>
      <details className="mb-4 text-xs text-muted-foreground">
        <summary className="w-fit cursor-pointer underline underline-offset-4">
          Technical details
        </summary>
        <p className="mt-2 max-w-prose">
          Local launches use a same-machine handshake. The token is held in localStorage and in the
          active profile's <code>bridge-token</code> file. The daemon stores that file with mode
          0600.
        </p>
      </details>
      <div className="space-y-4">
        {error ? (
          <Alert variant="destructive" className="px-3 py-2 text-xs">
            {error}
          </Alert>
        ) : null}
        <div className="space-y-1">
          <Label htmlFor="token-input">Token</Label>
          <div className="flex gap-2">
            <Input
              id="token-input"
              type={reveal ? "text" : "password"}
              value={draft}
              onChange={(e) => setDraft(e.target.value)}
              placeholder="paste bridge token here"
              className="font-mono"
              autoCapitalize="none"
              autoCorrect="off"
              spellCheck={false}
              disabled={checking}
            />
            <Button
              variant="outline"
              size="icon"
              type="button"
              onClick={() => setReveal((r) => !r)}
              aria-label={reveal ? "Hide token" : "Show token"}
            >
              {reveal ? <EyeOff className="size-3" /> : <Eye className="size-3" />}
            </Button>
          </div>
        </div>
        <div className="flex gap-2">
          <Button type="submit" disabled={!draft.trim() || checking}>
            {checking ? "Connecting…" : "Save and connect"}
          </Button>
          <Button
            variant="outline"
            type="button"
            disabled={!hasToken || checking}
            onClick={() => {
              clearToken();
              window.location.reload();
            }}
          >
            Clear token
          </Button>
        </div>
      </div>
    </form>
  );
}
