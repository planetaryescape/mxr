/*
 * Compose preferences: editor, undo-send window, default sending account
 * and signature blocks (stored in the daemon, shared with CLI compose).
 */

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { toast } from "sonner";

import { ConfirmDelete, Field, SelectSetting } from "./settingsParts";
import { apiFetch } from "@/api/client";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { fetchAccounts, setDefaultAccount } from "@/features/accounts/api";
import { useUiPrefs, type ComposeEditor, type UndoSendSeconds } from "@/state/uiPrefsStore";

interface Signature {
  id: string;
  name: string;
  body: string;
  created_at?: string;
  updated_at?: string;
}

interface SignatureDefault {
  kind: "new" | "reply";
  account_id?: string | null;
  from_email?: string | null;
  signature: Signature;
}

export function ComposeSettingsSection() {
  const qc = useQueryClient();
  const composeEditor = useUiPrefs((state) => state.composeEditor);
  const setComposeEditor = useUiPrefs((state) => state.setComposeEditor);
  const undoSendSeconds = useUiPrefs((state) => state.undoSendSeconds);
  const setUndoSendSeconds = useUiPrefs((state) => state.setUndoSendSeconds);
  const accounts = useQuery({
    queryKey: ["accounts"],
    queryFn: fetchAccounts,
    staleTime: 60_000,
  });
  const signatures = useQuery({
    queryKey: ["signatures"],
    queryFn: fetchSignatures,
    staleTime: 60_000,
  });
  const defaults = useQuery({
    queryKey: ["signature-defaults"],
    queryFn: fetchSignatureDefaults,
    staleTime: 60_000,
  });
  const [signatureName, setSignatureName] = useState("");
  const [signatureBody, setSignatureBody] = useState("");
  const makeDefault = useMutation({
    mutationFn: (key: string) => setDefaultAccount(key),
    onSuccess: () => {
      toast.success("Default account updated");
      void qc.invalidateQueries({ queryKey: ["accounts"] });
    },
    onError: (error) =>
      toast.error("Default account update failed", { description: error.message }),
  });
  const save = useMutation({
    mutationFn: () => saveSignature(signatureName.trim(), signatureBody),
    onSuccess: () => {
      toast.success("Signature saved");
      setSignatureName("");
      setSignatureBody("");
      void qc.invalidateQueries({ queryKey: ["signatures"] });
    },
    onError: (error) => toast.error("Signature save failed", { description: error.message }),
  });
  const remove = useMutation({
    mutationFn: deleteSignature,
    onSuccess: () => {
      toast.success("Signature deleted");
      void qc.invalidateQueries({ queryKey: ["signatures"] });
      void qc.invalidateQueries({ queryKey: ["signature-defaults"] });
    },
    onError: (error) => toast.error("Signature delete failed", { description: error.message }),
  });
  const setDefaultSignature = useMutation({
    mutationFn: setSignatureDefault,
    onSuccess: () => {
      toast.success("Signature default updated");
      void qc.invalidateQueries({ queryKey: ["signature-defaults"] });
    },
    onError: (error) =>
      toast.error("Signature default update failed", { description: error.message }),
  });

  const rows = accounts.data?.accounts ?? [];
  const signatureRows = signatures.data?.signatures ?? [];
  const defaultRows = defaults.data?.defaults ?? [];

  return (
    <div className="space-y-6">
      <div className="space-y-4">
        <section>
          <SelectSetting<ComposeEditor>
            label="Editor"
            description="CodeMirror with vim keys, or a rich-text editor."
            value={composeEditor}
            options={[
              { value: "codemirror-vim", label: "CodeMirror (vim)" },
              { value: "tiptap", label: "Rich text" },
            ]}
            onChange={setComposeEditor}
          />
          <SelectSetting
            label="Undo send window"
            description="Time to cancel after pressing send, from z or the toast."
            value={String(undoSendSeconds)}
            options={[
              { value: "0", label: "Send immediately" },
              { value: "5", label: "5 seconds" },
              { value: "10", label: "10 seconds" },
              { value: "30", label: "30 seconds" },
            ]}
            onChange={(value) => setUndoSendSeconds(Number(value) as UndoSendSeconds)}
          />
        </section>

        <section className="space-y-3 border-b border-border/60 pb-6">
          <div>
            <h2 className="font-mono text-[10.5px] uppercase tracking-[0.12em] text-muted-foreground">
              Default account
            </h2>
            <p className="mt-1 text-xs text-muted-foreground">
              Used for new drafts unless a reply or sender choice overrides it.
            </p>
          </div>
          <div className="divide-y divide-border">
            {accounts.isLoading ? (
              <div className="py-2 text-xs text-muted-foreground">Loading accounts...</div>
            ) : rows.length === 0 ? (
              <div className="py-2 text-xs text-muted-foreground">No accounts configured.</div>
            ) : (
              rows.map((account) => {
                const key = account.key ?? account.account_id;
                return (
                  <div
                    key={account.account_id}
                    className="flex items-center justify-between gap-3 py-2 text-xs"
                  >
                    <div>
                      <div className="font-medium">{account.name || account.email}</div>
                      <div className="font-mono text-2xs text-muted-foreground">
                        {account.email}
                      </div>
                    </div>
                    {account.is_default ? (
                      <Badge variant="secondary">default</Badge>
                    ) : (
                      <Button
                        variant="outline"
                        size="sm"
                        disabled={makeDefault.isPending}
                        aria-label={`Use ${account.name || account.email} as compose default`}
                        onClick={() => makeDefault.mutate(key)}
                      >
                        Use default
                      </Button>
                    )}
                  </div>
                );
              })
            )}
          </div>
        </section>

        <section className="space-y-4">
          <div>
            <h2 className="font-mono text-[10.5px] uppercase tracking-[0.12em] text-muted-foreground">
              Signatures
            </h2>
            <p className="mt-1 text-xs text-muted-foreground">
              Signature blocks are stored in the daemon and shared with CLI compose.
            </p>
          </div>
          <div className="grid max-w-xl gap-3 [&>button]:justify-self-start">
            <Field label="Signature name">
              <Input
                aria-label="Signature name"
                value={signatureName}
                onChange={(event) => setSignatureName(event.target.value)}
                placeholder="sig"
              />
            </Field>
            <Field label="Signature body">
              <Textarea
                aria-label="Signature body"
                value={signatureBody}
                onChange={(event) => setSignatureBody(event.target.value)}
                className="min-h-24"
                placeholder={"Best,\nYou"}
              />
            </Field>
            <Button
              disabled={!signatureName.trim() || !signatureBody.trim() || save.isPending}
              onClick={() => save.mutate()}
            >
              Save signature
            </Button>
          </div>
          <div className="divide-y divide-border">
            {signatures.isLoading ? (
              <div className="py-2 text-xs text-muted-foreground">Loading signatures...</div>
            ) : signatureRows.length === 0 ? (
              <div className="py-2 text-xs text-muted-foreground">No signatures yet.</div>
            ) : (
              signatureRows.map((signature) => (
                <div
                  key={signature.id}
                  className="grid gap-3 py-3 text-xs md:grid-cols-[1fr_auto] md:items-center"
                >
                  <div>
                    <div className="font-medium">{signature.name}</div>
                    <pre className="mt-1 whitespace-pre-wrap text-2xs text-muted-foreground">
                      {signature.body}
                    </pre>
                    <div className="mt-1 flex flex-wrap gap-1.5">
                      {defaultRows
                        .filter((item) => item.signature.name === signature.name)
                        .map((item) => (
                          <Badge key={item.kind} variant="outline">
                            default {item.kind}
                          </Badge>
                        ))}
                    </div>
                  </div>
                  <div className="flex flex-wrap justify-end gap-2">
                    <Button
                      variant="outline"
                      size="sm"
                      disabled={setDefaultSignature.isPending}
                      aria-label={`Use ${signature.name} for new messages`}
                      onClick={() =>
                        setDefaultSignature.mutate({ name: signature.name, kind: "new" })
                      }
                    >
                      New default
                    </Button>
                    <Button
                      variant="outline"
                      size="sm"
                      disabled={setDefaultSignature.isPending}
                      aria-label={`Use ${signature.name} for replies`}
                      onClick={() =>
                        setDefaultSignature.mutate({ name: signature.name, kind: "reply" })
                      }
                    >
                      Reply default
                    </Button>
                    <ConfirmDelete
                      what={`signature ${signature.name}`}
                      description="New drafts stop using it. Drafts that already contain it keep their text."
                      disabled={remove.isPending}
                      onConfirm={() => remove.mutate(signature.name)}
                    />
                  </div>
                </div>
              ))
            )}
          </div>
        </section>
      </div>
    </div>
  );
}

function fetchSignatures() {
  return apiFetch<{ signatures: Signature[] }>("/api/v1/mail/signatures");
}

function fetchSignatureDefaults() {
  return apiFetch<{ defaults: SignatureDefault[] }>("/api/v1/mail/signature-defaults");
}

function saveSignature(name: string, body: string) {
  return apiFetch<{ signature: Signature }>("/api/v1/mail/signatures", {
    method: "POST",
    body: { name, body },
  });
}

function deleteSignature(name: string) {
  return apiFetch<unknown>(`/api/v1/mail/signatures/${encodeURIComponent(name)}`, {
    method: "DELETE",
  });
}

function setSignatureDefault(input: { name: string; kind: "new" | "reply" }) {
  return apiFetch<unknown>("/api/v1/mail/signatures/default", {
    method: "POST",
    body: input,
  });
}
