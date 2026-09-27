import { keepPreviousData, useQueries, useQuery, useQueryClient } from "@tanstack/react-query";
import { useCallback, useEffect, useState } from "react";

import {
  defaultChoice,
  resolveTime,
  timeResolveQueryKey,
  type ResolvedTime,
  type TimeChoice,
  type TimeResolution,
  type TimeResolveError,
} from "./api";

/** Short enough to feel live, long enough to skip most keystrokes. */
const DEBOUNCE_MS = 80;
/** Relative phrases ("in 2h") drift while a dialog sits open. */
const REFRESH_MS = 30_000;

/** Query options for resolving `input`, shared by every caller. */
export function timeResolveQuery(input: string) {
  return {
    queryKey: timeResolveQueryKey(input),
    queryFn: ({ signal }: { signal: AbortSignal }) => resolveTime(input, signal),
    staleTime: 0,
  };
}

export interface ResolvedPreset {
  /** The reading the preset stores, once the daemon has answered. */
  choice: TimeChoice | null;
  failed: boolean;
}

/**
 * Resolve fixed preset phrases ("tomorrow 9am", "in 3 days") so each can show
 * and store its exact time. Reopening within half a minute reuses answers.
 */
export function useResolvedPresets(
  presets: readonly { input: string }[],
  { enabled = true }: { enabled?: boolean } = {},
): ResolvedPreset[] {
  const answers = useQueries({
    queries: presets.map((preset) => ({
      ...timeResolveQuery(preset.input),
      enabled,
      staleTime: 30_000,
    })),
  });
  return answers.map((answer) => ({
    choice: defaultChoice(answer.data),
    failed: answer.isError,
  }));
}

export interface NaturalTimeState {
  /** The text in the field. */
  value: string;
  setValue: (value: string) => void;
  /** Resolution for exactly `value`; null while it is being worked out. */
  resolution: TimeResolution | null;
  /** The latest answer, kept while the next one loads so the preview and
   * highlight don't flicker. May belong to an older `value`. */
  last: ResolvedTime | null;
  error: TimeResolveError | null;
  /** The bridge couldn't be reached, as opposed to a phrase it didn't get. */
  failure: Error | null;
  pending: boolean;
  choiceIndex: number;
  setChoiceIndex: (index: number) => void;
  /** The reading Enter would store. */
  selected: TimeChoice | null;
  /** There is text and the daemon hasn't rejected it. */
  canCommit: boolean;
  /**
   * The choice to store for the current text. Uses the fresh preview when
   * there is one; otherwise resolves now and takes the default reading, so a
   * fast Enter never stores something other than what the preview then shows.
   */
  commit: () => Promise<TimeChoice | null>;
  reset: () => void;
}

export function useNaturalTime({ enabled = true }: { enabled?: boolean } = {}): NaturalTimeState {
  const queryClient = useQueryClient();
  const [value, setValueState] = useState("");
  const [debounced, setDebounced] = useState("");
  const [choiceIndex, setChoiceIndex] = useState(0);

  useEffect(() => {
    const handle = window.setTimeout(() => setDebounced(value), DEBOUNCE_MS);
    return () => window.clearTimeout(handle);
  }, [value]);

  const active = enabled && debounced.trim().length > 0;
  const query = useQuery({
    // Keyed by the exact text, so a slow answer for "fri" can never be
    // shown as the answer for "fri 3".
    ...timeResolveQuery(debounced),
    enabled: active,
    placeholderData: keepPreviousData,
    gcTime: 60_000,
    refetchInterval: REFRESH_MS,
    retry: false,
  });

  const data: ResolvedTime | undefined = active ? query.data : undefined;
  const fresh = data !== undefined && data.input === value && !query.isPlaceholderData;
  const resolution = fresh ? (data.resolution ?? null) : null;
  const error = fresh ? (data.error ?? null) : null;
  const choices = resolution?.choices ?? [];
  const clampedIndex = Math.min(choiceIndex, Math.max(choices.length - 1, 0));
  const selected = choices[clampedIndex] ?? null;

  const setValue = useCallback((next: string) => {
    setValueState(next);
    setChoiceIndex(0);
  }, []);

  const reset = useCallback(() => {
    setValueState("");
    setDebounced("");
    setChoiceIndex(0);
  }, []);

  const commit = useCallback(async (): Promise<TimeChoice | null> => {
    if (!value.trim()) return null;
    if (selected) return selected;
    setDebounced(value);
    return defaultChoice(await queryClient.fetchQuery(timeResolveQuery(value)));
  }, [queryClient, selected, value]);

  return {
    value,
    setValue,
    resolution,
    last: data ?? null,
    error,
    failure: active && query.isError ? query.error : null,
    pending: value.trim().length > 0 && !fresh,
    choiceIndex: clampedIndex,
    setChoiceIndex,
    selected,
    canCommit: value.trim().length > 0 && error === null,
    commit,
    reset,
  };
}
