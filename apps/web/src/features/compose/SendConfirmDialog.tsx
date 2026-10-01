/*
 * Pre-send confirmation, opened when the daemon's safety check found issues
 * (or could not run). States exactly who receives the message and from
 * which address, shows the verdict and each issue, and never sends a
 * blocked draft without an explicit override (TUI parity: Ctrl-O).
 */

import { AlertTriangle, Loader2, Send, ShieldAlert, ShieldCheck, UserPlus } from "lucide-react";
import { useEffect, useId, useState } from "react";

import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog";
import { Alert, AlertDescription } from "@/components/ui/alert";
import { Badge } from "@/components/ui/badge";
import { Checkbox } from "@/components/ui/checkbox";
import { Label } from "@/components/ui/label";
import type { ComposeFrontmatter, DraftSafetyReport, SuggestedCollaborator } from "./api";
import { DraftQualityBadges } from "./DraftQualityBadges";
import { splitAddresses } from "./session/composeDraft";
import type { DraftSuggestionResponse } from "./types";

interface SendConfirmDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  frontmatter: ComposeFrontmatter;
  /** Address the message goes out from (the selected alias or account). */
  fromAddress: string;
  suggestion: DraftSuggestionResponse | null;
  sending: boolean;
  safetyReport: DraftSafetyReport | null;
  safetyCheckError: string | null;
  collaborators: SuggestedCollaborator[];
  onAddCc: (email: string) => void;
  /** `override` is true only after the user explicitly overrode a block. */
  onConfirm: (override: boolean) => void;
}

export function SendConfirmDialog({
  open,
  onOpenChange,
  frontmatter,
  fromAddress,
  suggestion,
  sending,
  safetyReport,
  safetyCheckError,
  collaborators,
  onAddCc,
  onConfirm,
}: SendConfirmDialogProps) {
  const overrideId = useId();
  const [overridden, setOverridden] = useState(false);
  useEffect(() => {
    if (!open) setOverridden(false);
  }, [open]);

  const blocked = safetyReport ? !safetyReport.allowed : false;
  const overridable =
    blocked && Boolean(safetyReport?.issues.some((issue) => issue.override_token));
  const canSend = !sending && (!blocked || (overridable && overridden));

  function confirm(override: boolean) {
    if (sending) return;
    if (blocked && !(overridable && override)) return;
    onConfirm(override);
  }

  const recipientRows = (
    [
      ["To", frontmatter.to],
      ["Cc", frontmatter.cc],
      ["Bcc", frontmatter.bcc],
    ] as const
  )
    .map(([label, value]) => [label, splitAddresses(value)] as const)
    .filter(([, addresses]) => addresses.length > 0);

  return (
    <AlertDialog open={open} onOpenChange={onOpenChange}>
      <AlertDialogContent
        onKeyDown={(event) => {
          if (event.key === "Enter" && !event.shiftKey) {
            event.preventDefault();
            if (canSend) confirm(overridden);
            return;
          }
          if (overridable && event.ctrlKey && event.key.toLowerCase() === "o") {
            event.preventDefault();
            setOverridden(true);
            confirm(true);
          }
        }}
      >
        <AlertDialogHeader>
          <AlertDialogTitle className="flex items-center gap-2">
            {blocked ? "Send blocked by safety check" : "Send message?"}
            <SafetyVerdictBadge report={safetyReport} failed={Boolean(safetyCheckError)} />
          </AlertDialogTitle>
          <AlertDialogDescription>
            From <span className="font-mono text-foreground">{fromAddress}</span>
          </AlertDialogDescription>
        </AlertDialogHeader>

        <dl
          aria-label="Recipients"
          className="grid grid-cols-[2.5rem_minmax(0,1fr)] gap-x-2 gap-y-1 rounded-lg border border-border bg-muted px-3 py-2 text-xs"
        >
          {recipientRows.map(([label, addresses]) => (
            <div key={label} className="contents">
              <dt className="text-muted-foreground">{label}</dt>
              <dd className="min-w-0 break-words">{addresses.join(", ")}</dd>
            </div>
          ))}
          <dt className="text-muted-foreground">Subject</dt>
          <dd className="min-w-0 truncate">{frontmatter.subject.trim() || "(no subject)"}</dd>
        </dl>

        {safetyCheckError ? (
          <Alert variant="warning" className="flex items-center gap-2 px-3 py-2">
            <AlertTriangle className="size-3 shrink-0 text-warning" />
            <AlertDescription>Safety check unavailable: {safetyCheckError}</AlertDescription>
          </Alert>
        ) : null}
        {safetyReport && safetyReport.issues.length > 0 ? (
          <div className="space-y-2" role="alert">
            {safetyReport.issues.map((issue) => (
              <Alert
                key={`${issue.code}-${issue.message}`}
                variant={issue.severity === "blocker" ? "destructive" : "warning"}
                className="flex items-start gap-2 px-3 py-2"
              >
                <AlertTriangle
                  className={
                    issue.severity === "blocker"
                      ? "mt-0.5 size-3 shrink-0 text-destructive"
                      : "mt-0.5 size-3 shrink-0 text-warning"
                  }
                />
                <AlertDescription>
                  {issue.message}
                  {issue.detail ? (
                    <span className="block text-2xs text-muted-foreground">{issue.detail}</span>
                  ) : null}
                </AlertDescription>
              </Alert>
            ))}
          </div>
        ) : null}

        {collaborators.length > 0 ? (
          <div className="flex flex-wrap items-center gap-1.5 text-2xs text-muted-foreground">
            <span>Maybe cc:</span>
            {collaborators.map((collaborator) => (
              <button
                key={collaborator.email}
                type="button"
                className="inline-flex items-center gap-1 rounded-full border border-border bg-muted/40 px-2 py-0.5 text-foreground hover:border-primary/50 hover:bg-muted"
                title={collaborator.reason}
                aria-label={`Add ${collaborator.email} to Cc`}
                onClick={() => onAddCc(collaborator.email)}
              >
                <UserPlus className="size-3" />
                {collaborator.display_name || collaborator.email}
              </button>
            ))}
          </div>
        ) : null}

        {blocked ? (
          overridable ? (
            <div className="flex items-start gap-2 rounded-lg border border-destructive/30 bg-destructive/10 px-3 py-2">
              <Checkbox
                id={overrideId}
                checked={overridden}
                onCheckedChange={(value) => setOverridden(value === true)}
                className="mt-0.5"
              />
              <Label htmlFor={overrideId} className="text-xs leading-snug text-destructive">
                Override the block and send this once anyway (Ctrl+O)
              </Label>
            </div>
          ) : (
            <p className="text-xs text-destructive">
              This block can&apos;t be overridden. Edit the draft to fix it.
            </p>
          )
        ) : null}

        <DraftQualityBadges suggestion={suggestion} />
        <AlertDialogFooter>
          <AlertDialogCancel variant="outline" disabled={sending}>
            {blocked ? "Back to draft" : "Cancel"}
          </AlertDialogCancel>
          <AlertDialogAction
            disabled={!canSend}
            variant={blocked ? "destructive" : undefined}
            onClick={(event) => {
              event.preventDefault();
              confirm(overridden);
            }}
          >
            {sending ? <Loader2 className="size-3 animate-spin" /> : <Send className="size-3" />}
            {blocked ? "Send anyway" : "Send"}
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}

function SafetyVerdictBadge({
  report,
  failed,
}: {
  report: DraftSafetyReport | null;
  failed: boolean;
}) {
  if (failed || !report) {
    return (
      <Badge variant="warning">
        <ShieldAlert />
        Not checked
      </Badge>
    );
  }
  if (!report.allowed || report.verdict === "blocked") {
    return (
      <Badge variant="destructive">
        <ShieldAlert />
        Blocked
      </Badge>
    );
  }
  if (report.verdict === "warn" || report.issues.length > 0) {
    return (
      <Badge variant="warning">
        <ShieldAlert />
        Warning
      </Badge>
    );
  }
  return (
    <Badge variant="success">
      <ShieldCheck />
      Safe
    </Badge>
  );
}
