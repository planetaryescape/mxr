import { keepPreviousData, useQueries, useQuery, useQueryClient } from "@tanstack/react-query";
import { useCallback, useEffect, useRef, useState } from "react";

import {
  defaultChoice,
  resolveTime,
  timeResolveQueryKey,
  type ResolvedTime,
  type TimeChoice,
  type TimeResolution,
  type TimeResolveError,
} from "./api";

const DEBOUNCE_MS = 80;
const REFRESH_MS = 30_000;

export function timeResolveQuery(input: string) {
  return {
    queryKey: timeResolveQueryKey(input),
    queryFn: ({ signal }: { signal: AbortSignal }) => resolveTime(input, signal),
    staleTime: 0,
  };
}

export interface ResolvedPreset {
  choice: TimeChoice | null;
  failed: boolean;
}

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
  value: string;
  setValue: (value: string) => void;
  resolution: TimeResolution | null;
  last: ResolvedTime | null;
  error: TimeResolveError | null;
  failure: Error | null;
  pending: boolean;
  choiceIndex: number;
  setChoiceIndex: (index: number) => void;
  selected: TimeChoice | null;
  canCommit: boolean;
  commit: (use: (choice: TimeChoice) => void | Promise<void>) => Promise<void>;
  runExclusive: (task: () => void | Promise<void>) => Promise<void>;
  submitting: boolean;
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

  const inFlight = useRef(false);
  const [submitting, setSubmitting] = useState(false);

  const runExclusive = useCallback(async (task: () => void | Promise<void>) => {
    if (inFlight.current) return;
    inFlight.current = true;
    setSubmitting(true);
    try {
      await task();
    } finally {
      inFlight.current = false;
      setSubmitting(false);
    }
  }, []);

  const choose = useCallback(async (): Promise<TimeChoice | null> => {
    if (!value.trim()) return null;
    if (selected) return selected;
    setDebounced(value);
    const answer = await queryClient.fetchQuery(timeResolveQuery(value));
    if ((answer.resolution?.choices.length ?? 0) > 1) return null;
    return defaultChoice(answer);
  }, [queryClient, selected, value]);

  const commit = useCallback(
    (use: (choice: TimeChoice) => void | Promise<void>) =>
      runExclusive(async () => {
        const choice = await choose();
        if (choice) await use(choice);
      }),
    [choose, runExclusive],
  );

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
    canCommit: value.trim().length > 0 && error === null && !submitting,
    commit,
    runExclusive,
    submitting,
    reset,
  };
}
