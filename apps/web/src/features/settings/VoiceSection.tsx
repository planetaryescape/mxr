/*
 * The local profile of how you write (formality, sentence length), built
 * from your sent mail. Draft assist uses it to match your voice.
 */

import { useMutation, useQuery } from "@tanstack/react-query";
import { useState } from "react";
import { toast } from "sonner";

import { SettingRow } from "./settingsParts";
import { apiFetch } from "@/api/client";
import { FactList, PageError, PageSkeleton } from "@/components/PageParts";
import { Button } from "@/components/ui/button";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { fetchAccounts } from "@/features/accounts/api";
import { formatRelative, plural } from "@/lib/format";

interface UserVoiceProfile {
  account_id: string;
  formality_score: number;
  avg_sentence_len: number;
  msg_count_used: number;
  register_modes?: Array<{
    register: string;
    formality_score: number;
    avg_sentence_len: number;
    exemplar_message_ids?: string[];
  }>;
  computed_at: string;
}

function fetchVoice(accountId: string) {
  return apiFetch<{ profile: UserVoiceProfile }>(
    `/api/v1/platform/voice?account_id=${encodeURIComponent(accountId)}`,
  );
}

function rebuildVoice(accountId: string) {
  return apiFetch<{ profile: UserVoiceProfile }>(
    `/api/v1/platform/voice/rebuild?account_id=${encodeURIComponent(accountId)}`,
    { method: "POST" },
  );
}

export function VoiceSection() {
  const accounts = useQuery({ queryKey: ["accounts"], queryFn: fetchAccounts });
  const [picked, setPicked] = useState<string | null>(null);
  const accountId = picked ?? accounts.data?.accounts[0]?.account_id ?? null;
  const voice = useQuery({
    queryKey: ["user-voice", accountId],
    queryFn: () => fetchVoice(accountId ?? ""),
    enabled: Boolean(accountId),
    retry: false,
  });
  const rebuild = useMutation({
    mutationFn: () => rebuildVoice(accountId ?? ""),
    onSuccess: () => {
      toast.success("Voice profile rebuilt");
      void voice.refetch();
    },
    onError: (error) => toast.error("Voice rebuild failed", { description: error.message }),
  });
  const profile = voice.data?.profile;

  return (
    <div className="space-y-6">
      <SettingRow label="Account" description="Each account has its own voice profile.">
        <div className="flex items-center gap-2">
          <Select value={accountId ?? ""} onValueChange={setPicked}>
            <SelectTrigger className="h-8 w-56 text-xs" aria-label="Voice profile account">
              <SelectValue placeholder={accounts.isPending ? "Loading…" : "No accounts"} />
            </SelectTrigger>
            <SelectContent>
              {(accounts.data?.accounts ?? []).map((account) => (
                <SelectItem key={account.account_id} value={account.account_id} className="text-xs">
                  {account.email}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
          <Button
            variant="outline"
            size="sm"
            disabled={!accountId || rebuild.isPending}
            onClick={() => rebuild.mutate()}
          >
            {rebuild.isPending ? "Rebuilding…" : "Rebuild"}
          </Button>
        </div>
      </SettingRow>
      {!accountId ? null : voice.isPending ? (
        <PageSkeleton rows={3} label="Loading voice profile" />
      ) : voice.isError ? (
        <PageError
          title="No voice profile yet"
          error={
            new Error(
              `${voice.error.message}. A profile needs about 20 sent messages; rebuild once you have them.`,
            )
          }
          onRetry={() => void voice.refetch()}
        />
      ) : profile ? (
        <div className="space-y-5">
          <p className="font-mono text-2xs text-muted-foreground">
            Built {formatRelative(profile.computed_at)} from{" "}
            {plural(profile.msg_count_used, "sent message")}
          </p>
          <FactList
            facts={[
              ["Formality", formality(profile.formality_score)],
              ["Sentence length", `${profile.avg_sentence_len.toFixed(1)} words`],
              ...(profile.register_modes ?? []).map((mode): [string, string] => [
                mode.register,
                `${formality(mode.formality_score)}, ${mode.avg_sentence_len.toFixed(1)} words`,
              ]),
            ]}
          />
        </div>
      ) : null}
    </div>
  );
}

function formality(score: number): string {
  const tone = score >= 0.68 ? "formal" : score <= 0.38 ? "casual" : "neutral";
  return `${tone} (${score.toFixed(2)})`;
}
