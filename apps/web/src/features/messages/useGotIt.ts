import { useCallback, useEffect, useRef, useState } from "react";
import { toast } from "sonner";

import { refuseWhileDaemonDown } from "@/lib/daemonAvailability";
import { useUndo } from "@/state/undoStore";

import { previewAck, refreshMessages, sendAck, type AckPlan } from "./api";

export interface PendingAck {
  plan: AckPlan;
  endsAt: number;
}

export function useGotIt(scope: string, onSent?: (plan: AckPlan) => void) {
  const [pending, setPending] = useState<PendingAck | null>(null);
  const [loading, setLoading] = useState(false);
  const generation = useRef(0);
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const abort = useRef<AbortController | null>(null);
  const undoEntry = useRef<(() => void) | null>(null);
  const onSentRef = useRef(onSent);
  onSentRef.current = onSent;

  const stopAll = useCallback(() => {
    generation.current += 1;
    if (timer.current !== null) clearTimeout(timer.current);
    timer.current = null;
    abort.current?.abort();
    abort.current = null;
    const entry = undoEntry.current;
    if (entry) useUndo.getState().clearPendingSendCancel(entry);
    undoEntry.current = null;
  }, []);

  const send = useCallback(async (plan: AckPlan, mine: number) => {
    if (mine !== generation.current) return;
    const entry = undoEntry.current;
    if (entry) useUndo.getState().clearPendingSendCancel(entry);
    undoEntry.current = null;
    timer.current = null;
    setPending(null);
    try {
      await sendAck(plan);
      const to = plan.to[0]?.name ?? plan.to[0]?.email ?? "them";
      toast.success(`Got it sent to ${to}.`);
      onSentRef.current?.(plan);
    } catch (error) {
      toast.error("Couldn't send Got it", {
        description: error instanceof Error ? error.message : String(error),
      });
    } finally {
      await refreshMessages();
    }
  }, []);

  const start = useCallback(
    async (threadId: string) => {
      if (loading || refuseWhileDaemonDown("send Got it")) return;
      stopAll();
      setPending(null);
      const mine = generation.current;
      const controller = new AbortController();
      abort.current = controller;
      setLoading(true);
      let plan: AckPlan;
      try {
        plan = await previewAck(threadId, controller.signal);
      } catch (error) {
        if (mine === generation.current && !controller.signal.aborted) {
          toast.error("Couldn't prepare Got it", {
            description: error instanceof Error ? error.message : String(error),
          });
          setLoading(false);
        }
        return;
      }
      if (mine !== generation.current || controller.signal.aborted) return;
      abort.current = null;
      setLoading(false);
      if (!plan.preview_token) {
        toast.error("Couldn't prepare Got it", {
          description: "The daemon gave no preview to send. Nothing was sent.",
        });
        return;
      }
      const endsAt = Date.now() + plan.countdown_seconds * 1000;
      const cancel = () => {
        if (mine !== generation.current) return;
        stopAll();
        setPending(null);
        toast.info("Got it cancelled. Nothing was sent.");
      };
      undoEntry.current = cancel;
      useUndo.getState().setPendingSendCancel(cancel);
      setPending({ plan, endsAt });
      timer.current = setTimeout(() => void send(plan, mine), endsAt - Date.now());
    },
    [loading, send, stopAll],
  );

  useEffect(() => {
    setPending(null);
    setLoading(false);
    return stopAll;
  }, [scope, stopAll]);

  const undo = useCallback(() => {
    undoEntry.current?.();
  }, []);

  return { pending, loading, start, undo };
}
