import { useQuery } from "@tanstack/react-query";

import { apiFetch } from "@/api/client";

export interface InviteLocaleStrings {
  card_title: string;
  chip_label_accept: string;
  chip_label_tentative: string;
  chip_label_decline: string;
  state_label_accepted: string;
  state_label_tentative: string;
  state_label_declined: string;
  hint_change_response: string;
  hint_comment: string;
  banner_cancelled: string;
  banner_publish: string;
  banner_parse_warning: string;
  banner_updated: string;
  banner_counter: string;
}

export interface StatusLocaleStrings {
  invite_pending_accept: string;
  invite_pending_tentative: string;
  invite_pending_decline: string;
  invite_cancelled: string;
}

export interface LocaleBundle {
  code: string;
  invite: InviteLocaleStrings;
  status: StatusLocaleStrings;
}

const FALLBACK_LOCALE: LocaleBundle = {
  code: "en",
  invite: {
    card_title: "Calendar invite",
    chip_label_accept: "Accept",
    chip_label_tentative: "Maybe",
    chip_label_decline: "Decline",
    state_label_accepted: "✓ You accepted",
    state_label_tentative: "? You said maybe",
    state_label_declined: "✗ You declined",
    hint_change_response: "press ia/im/id to change",
    hint_comment: "Shift+iA/iM/iD to comment",
    banner_cancelled: "Event canceled by organizer",
    banner_publish: "Informational — no reply expected",
    banner_parse_warning: "Calendar invite could not be parsed",
    banner_updated: "Updated invite",
    banner_counter: "Counter-proposal received",
  },
  status: {
    invite_pending_accept: "Accepting invite — u to undo (1s)",
    invite_pending_tentative: "Tentatively accepting invite — u to undo (1s)",
    invite_pending_decline: "Declining invite — u to undo (1s)",
    invite_cancelled: "Cancelled — no reply sent",
  },
};

/// Fetches the daemon's active locale bundle once at mount and caches it via
/// TanStack Query. The bundle never changes within a daemon session, so a
/// long stale-time and infinite gcTime are appropriate.
export function useLocale(): LocaleBundle {
  const { data } = useQuery({
    queryKey: ["i18n"],
    queryFn: () => apiFetch<LocaleBundle>("/api/v1/i18n"),
    staleTime: Infinity,
    gcTime: Infinity,
  });
  return data ?? FALLBACK_LOCALE;
}
