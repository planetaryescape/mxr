import { startTransition, useEffectEvent } from "react";
import type { RefObject, SetStateAction } from "react";
import type {
  AccountsResponse,
  BridgeState,
  DiagnosticsResponse,
  FocusContext,
  LayoutMode,
  MailboxResponse,
  ReaderMode,
  RulesResponse,
  SearchMode,
  SearchResponse,
  SearchScope,
  SearchSort,
  SidebarItem,
  SidebarLens,
  SidebarPayload,
  ThreadResponse,
  WorkbenchScreen,
  WorkbenchShellPayload,
} from "../../shared/types";
import { fetchJson } from "./bridgeHttp";

type StateSetter<T> = (updater: SetStateAction<T>) => void;

export function useWorkbenchShellActions(props: {
  bridge: BridgeState;
  deferredSearchQuery: string;
  searchScope: SearchScope;
  searchMode: SearchMode;
  searchSort: SearchSort;
  searchExplain: boolean;
  sidebar: SidebarPayload;
  selectedRow: { thread_id: string } | null;
  screen: WorkbenchScreen;
  setBridge: StateSetter<BridgeState>;
  setShell: StateSetter<WorkbenchShellPayload>;
  setSidebar: StateSetter<SidebarPayload>;
  setMailbox: StateSetter<MailboxResponse["mailbox"]>;
  setScreen: StateSetter<WorkbenchScreen>;
  setLayoutMode: StateSetter<LayoutMode>;
  setThread: StateSetter<ThreadResponse | null>;
  setSelectedMailboxThreadId: StateSetter<string | null>;
  setSelectedSearchThreadId: StateSetter<string | null>;
  setShowInboxZero: StateSetter<boolean>;
  setWorkbenchReady: StateSetter<boolean>;
  searchState: SearchResponse;
  setSearchState: StateSetter<SearchResponse>;
  setRulesState: StateSetter<RulesResponse>;
  setSelectedRuleId: StateSetter<string | null>;
  setAccountsState: StateSetter<AccountsResponse>;
  setSelectedAccountId: StateSetter<string | null>;
  setDiagnosticsState: StateSetter<DiagnosticsResponse | null>;
  setFocusContext: StateSetter<FocusContext>;
  setCommandPaletteOpen: StateSetter<boolean>;
  setCommandQuery: StateSetter<string>;
  setReaderMode: StateSetter<ReaderMode>;
  setSignatureExpanded: StateSetter<boolean>;
  searchInputRef: RefObject<HTMLInputElement | null>;
}) {
  const loadMailbox = useEffectEvent(
    async (lens?: SidebarLens, options?: { preserveReader?: boolean }) => {
      if (props.bridge.kind !== "ready") {
        return;
      }

      const payload = await fetchJson<MailboxResponse>(
        props.bridge.baseUrl,
        props.bridge.authToken,
        mailboxPath(lens),
      );
      startTransition(() => {
        props.setShell(payload.shell);
        props.setSidebar(payload.sidebar);
        props.setMailbox(payload.mailbox);
        if (!options?.preserveReader) {
          props.setScreen("mailbox");
          props.setLayoutMode("twoPane");
          props.setThread(null);
        }
        props.setSelectedMailboxThreadId(
          payload.mailbox.groups.flatMap((group) => group.rows)[0]?.thread_id ?? null,
        );
        props.setShowInboxZero(
          payload.mailbox.groups.length === 0 && payload.mailbox.lensLabel === "Inbox",
        );
        props.setWorkbenchReady(true);
      });
    },
  );

  const loadSearch = useEffectEvent(async () => {
    if (props.bridge.kind !== "ready") {
      return;
    }

    const params = new URLSearchParams();
    if (props.deferredSearchQuery) {
      params.set("q", props.deferredSearchQuery);
    }
    params.set("scope", props.searchScope);
    params.set("mode", props.searchMode);
    params.set("sort", props.searchSort);
    if (props.searchExplain) {
      params.set("explain", "true");
    }
    const payload = await fetchJson<SearchResponse>(
      props.bridge.baseUrl,
      props.bridge.authToken,
      `/search?${params.toString()}`,
    );
    startTransition(() => {
      props.setSearchState(payload);
      props.setSelectedSearchThreadId(
        payload.groups.flatMap((group) => group.rows)[0]?.thread_id ?? null,
      );
    });
  });

  const loadMoreSearch = useEffectEvent(async () => {
    if (props.bridge.kind !== "ready") {
      return;
    }

    const currentTotal = props.searchState.groups.flatMap((g) => g.rows).length;
    const params = new URLSearchParams();
    if (props.deferredSearchQuery) {
      params.set("q", props.deferredSearchQuery);
    }
    params.set("scope", props.searchScope);
    params.set("mode", props.searchMode);
    params.set("sort", props.searchSort);
    params.set("limit", String(currentTotal + 50));
    if (props.searchExplain) {
      params.set("explain", "true");
    }
    const payload = await fetchJson<SearchResponse>(
      props.bridge.baseUrl,
      props.bridge.authToken,
      `/search?${params.toString()}`,
    );
    startTransition(() => {
      props.setSearchState(payload);
    });
  });

  const loadThread = useEffectEvent(async (threadId: string) => {
    if (props.bridge.kind !== "ready") {
      return;
    }

    const payload = await fetchJson<ThreadResponse>(
      props.bridge.baseUrl,
      props.bridge.authToken,
      `/thread/${threadId}`,
    );
    startTransition(() => {
      props.setThread(payload);
      props.setReaderMode("auto");
      props.setSignatureExpanded(false);
    });
  });

  const loadRules = useEffectEvent(async () => {
    if (props.bridge.kind !== "ready") {
      return;
    }
    const payload = await fetchJson<RulesResponse>(
      props.bridge.baseUrl,
      props.bridge.authToken,
      "/rules",
    );
    startTransition(() => {
      props.setRulesState(payload);
      props.setSelectedRuleId((current) => {
        const stillExists = payload.rules.some(
          (rule) => String(rule.id ?? rule.name ?? "") === current,
        );
        if (stillExists) {
          return current;
        }
        return String(payload.rules[0]?.id ?? payload.rules[0]?.name ?? "") || null;
      });
    });
  });

  const loadAccounts = useEffectEvent(async () => {
    if (props.bridge.kind !== "ready") {
      return;
    }
    const payload = await fetchJson<AccountsResponse>(
      props.bridge.baseUrl,
      props.bridge.authToken,
      "/accounts",
    );
    startTransition(() => {
      props.setAccountsState(payload);
      props.setSelectedAccountId((current) => {
        const stillExists = payload.accounts.some((account) => account.account_id === current);
        if (stillExists) {
          return current;
        }
        return payload.accounts[0]?.account_id ?? null;
      });
    });
  });

  const loadDiagnostics = useEffectEvent(async () => {
    if (props.bridge.kind !== "ready") {
      return;
    }
    const payload = await fetchJson<DiagnosticsResponse>(
      props.bridge.baseUrl,
      props.bridge.authToken,
      "/diagnostics",
    );
    startTransition(() => props.setDiagnosticsState(payload));
  });

  const openThread = useEffectEvent(() => {
    if (!props.selectedRow) {
      return;
    }
    props.setLayoutMode("threePane");
    props.setFocusContext("reader");
    if (props.screen === "search") {
      props.setSelectedSearchThreadId(props.selectedRow.thread_id);
      return;
    }
    props.setSelectedMailboxThreadId(props.selectedRow.thread_id);
  });

  const closeReader = useEffectEvent(() => {
    props.setLayoutMode("twoPane");
    props.setFocusContext(props.screen === "search" ? "search" : "mailList");
  });

  const refreshBridge = useEffectEvent(async () => {
    const next = await window.mxrDesktop.retryBridge();
    props.setBridge(next);
  });

  const applySidebarLens = useEffectEvent(async (item: SidebarItem, options?: { preserveFocus?: boolean }) => {
    props.setSidebar((current) => setActiveSidebarItem(current, item.id));
    if (!options?.preserveFocus) {
      props.setFocusContext("mailList");
    }
    await loadMailbox(item.lens);
  });

  const applySidebarLensById = useEffectEvent(async (itemId: string) => {
    const item = findSidebarItem(props.sidebar, itemId);
    if (!item) {
      return;
    }
    await applySidebarLens(item);
  });

  const switchScreen = useEffectEvent((next: WorkbenchScreen) => {
    props.setScreen(next);
    props.setCommandPaletteOpen(false);
    props.setCommandQuery("");
    if (next !== "mailbox") {
      props.setLayoutMode("twoPane");
      props.setThread(null);
    }
    if (next === "search") {
      props.setFocusContext("search");
      queueMicrotask(() => props.searchInputRef.current?.focus());
      return;
    }
    props.setFocusContext(next === "mailbox" ? "mailList" : "sidebar");
  });

  return {
    loadMailbox,
    loadSearch,
    loadMoreSearch,
    loadThread,
    loadRules,
    loadAccounts,
    loadDiagnostics,
    openThread,
    closeReader,
    refreshBridge,
    applySidebarLens,
    applySidebarLensById,
    switchScreen,
  };
}

function setActiveSidebarItem(sidebar: SidebarPayload, activeId: string): SidebarPayload {
  return {
    sections: sidebar.sections.map((section) => ({
      ...section,
      items: section.items.map((item) => ({
        ...item,
        active: item.id === activeId,
      })),
    })),
  };
}

function findSidebarItem(sidebar: SidebarPayload, itemId: string): SidebarItem | null {
  for (const section of sidebar.sections) {
    const match = section.items.find((item) => item.id === itemId);
    if (match) {
      return match;
    }
  }
  return null;
}

function mailboxPath(lens?: SidebarLens) {
  if (!lens) {
    return "/mailbox";
  }

  const params = new URLSearchParams();
  params.set("lens_kind", lens.kind);

  if (lens.labelId) {
    params.set("label_id", lens.labelId);
  }
  if (lens.savedSearch) {
    params.set("saved_search", lens.savedSearch);
  }
  if (lens.senderEmail) {
    params.set("sender_email", lens.senderEmail);
  }

  return `/mailbox?${params.toString()}`;
}
