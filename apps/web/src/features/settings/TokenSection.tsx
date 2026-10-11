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
    <div>
      <p className="mb-5 max-w-prose text-[13px] text-muted-foreground">
        mxr web needs local access to connect to your mail app. Most local launches set this up
        automatically. If access expires, paste the token from your active mxr profile here.
      </p>
      <details className="mb-4 text-xs text-muted-foreground">
        <summary className="w-fit cursor-pointer underline underline-offset-4">
          Technical details
        </summary>
        <p className="mt-2 max-w-prose">
          Local launches use a same-machine handshake. The token is held in localStorage and in the
          daemon's active profile config directory (mode 0600).
        </p>
      </details>
      <div className="space-y-4">
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
      </div>
    </div>
  );
}
