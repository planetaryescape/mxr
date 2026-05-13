import { useEffect } from "react";

import { fetchBridgeHealth } from "@/lib/bridgeHealth";
import { evaluateProtocolCompatibility } from "@/lib/protocolCompatibility";
import { useConnectionStore } from "@/state/connectionStore";

export function useProtocolCompatibilityBootstrap(): void {
  const setState = useConnectionStore((state) => state.setState);

  useEffect(() => {
    const controller = new AbortController();

    fetchBridgeHealth(controller.signal)
      .then((health) => {
        setState({
          protocolMismatch: evaluateProtocolCompatibility(health),
          protocolCheckedAt: Date.now(),
          protocolCheckError: undefined,
        });
      })
      .catch((error: Error) => {
        if (controller.signal.aborted) return;
        setState({
          protocolCheckedAt: Date.now(),
          protocolCheckError: error.message,
        });
      });

    return () => controller.abort();
  }, [setState]);
}
