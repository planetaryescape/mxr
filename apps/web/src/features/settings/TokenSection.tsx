import { Eye, EyeOff } from "lucide-react";
import { useState } from "react";

import { Alert } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { useBridgeToken } from "@/hooks/useBridgeToken";

export function TokenSection() {
  const { token, setToken, clearToken, hasToken } = useBridgeToken();
  const [draft, setDraft] = useState(token);
  const [reveal, setReveal] = useState(false);
  const reason = new URLSearchParams(window.location.search).get("reason");

  return (
    <div className="flex h-full w-full flex-col">
      <div className="border-b border-border px-6 py-4">
        <h1 className="text-md font-semibold">Bridge token</h1>
        <p className="mt-1 max-w-prose text-2xs text-muted-foreground">
          The web app authenticates to the local <code>mxr</code> daemon using a bearer token. Local
          launches normally fetch it through the same-machine handshake and persist it to
          <code>localStorage</code>. If you see <em>no token</em> or 401 errors, paste the token
          from
          <code>~/.config/mxr/bridge-token</code> here.
        </p>
      </div>
      <div className="space-y-4 p-6">
        {reason === "expired" ? (
          <Alert variant="destructive" className="px-3 py-2 text-xs">
            The bridge token was rejected. Paste a valid token to reconnect.
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
            />
            <Button
              variant="outline"
              size="icon"
              onClick={() => setReveal((r) => !r)}
              aria-label={reveal ? "Hide token" : "Show token"}
            >
              {reveal ? <EyeOff className="size-3" /> : <Eye className="size-3" />}
            </Button>
          </div>
        </div>
        <div className="flex gap-2">
          <Button
            onClick={() => {
              setToken(draft.trim());
              window.location.reload();
            }}
            disabled={!draft.trim() || draft === token}
          >
            Save and reload
          </Button>
          <Button
            variant="outline"
            disabled={!hasToken}
            onClick={() => {
              clearToken();
              window.location.reload();
            }}
          >
            Clear token
          </Button>
        </div>
        <p className="text-2xs text-muted-foreground">
          Storage location: <code>localStorage["mxr.bridgeToken"]</code>. The token is also held on
          disk by the daemon at <code>~/.config/mxr/bridge-token</code> (mode 0600).
        </p>
      </div>
    </div>
  );
}
