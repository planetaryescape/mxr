/*
 * Got it (`.`): fetch the exact acknowledgement from the daemon (a dry
 * run), show it with a countdown, then send exactly that text. Undo, the
 * button or the global `u`, cancels before anything leaves. The daemon
 * refuses a send whose text differs from the preview, so what was shown is
 * what goes.
 */

import { useCallback, useEffect, useRef, useState } from "react";
import { toast } from "sonner";

import { refuseWhileDaemonDown } from "@/lib/daemonAvailability";
import { useUndo } from "@/state/undoStore";

import { ack, refreshMessages, type AckPlan } from "./api";

export interface PendingAck {
  plan: AckPlan;
  /** Epoch ms when it sends. */
  endsAt: number;
}

export function useGotIt(onSent?: (plan: AckPlan) => void) {
  const [pending, setPending] = useState<PendingAck | null>(null);
  const [loading, setLoading] = useState(false);
  const timer = useRef<number | null>(null);
  const cancelRef = useRef<(() => void) | null>(null);
  const onSentRef = useRef(onSent);
  onSentRef.current = onSent;

  const clearTimer = () => {
    if (timer.current !== null) window.clearTimeout(timer.current);
    timer.current = null;
  };

  const cancel = useCallback(() => {
    clearTimer();
    const registered = cancelRef.current;
    if (registered) useUndo.getState().clearPendingSendCancel(registered);
    cancelRef.current = null;
    setPending(null);
  }, []);

  const send = useCallback(
    async (plan: AckPlan) => {
      const registered = cancelRef.current;
      if (registered) useUndo.getState().clearPendingSendCancel(registered);
      cancelRef.current = null;
      setPending(null);
      try {
        await ack(plan.thread_id, false, plan.text);
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
    },
    [],
  );

  const start = useCallback(
    async (threadId: string) => {
      if (loading || refuseWhileDaemonDown("send Got it")) return;
      cancel();
      setLoading(true);
      try {
        const plan = await ack(threadId, true);
        const endsAt = Date.now() + plan.countdown_seconds * 1000;
        const stop = () => {
          // Retire itself, so a later `u` reaches the next undo, not this.
          useUndo.getState().clearPendingSendCancel(stop);
          clearTimer();
          cancelRef.current = null;
          setPending(null);
          toast.info("Got it cancelled. Nothing was sent.");
        };
        cancelRef.current = stop;
        useUndo.getState().setPendingSendCancel(stop);
        setPending({ plan, endsAt });
        timer.current = window.setTimeout(() => void send(plan), endsAt - Date.now());
      } catch (error) {
        toast.error("Couldn't prepare Got it", {
          description: error instanceof Error ? error.message : String(error),
        });
      } finally {
        setLoading(false);
      }
    },
    [cancel, loading, send],
  );

  // Leaving the page cancels a Got it still counting down: nothing sends
  // from a view that is gone.
  useEffect(() => cancel, [cancel]);

  const undo = useCallback(() => {
    const stop = cancelRef.current;
    if (stop) {
      useUndo.getState().clearPendingSendCancel(stop);
      stop();
    }
  }, []);

  return { pending, loading, start, undo };
}
