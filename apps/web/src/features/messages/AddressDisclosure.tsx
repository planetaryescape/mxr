import { Copy } from "lucide-react";
import { useRef } from "react";
import { toast } from "sonner";

import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";

export function AddressDisclosure({
  trigger,
  label,
  heading,
  name,
  address,
}: {
  trigger: string;
  label: string;
  heading: string;
  name?: string | null;
  address: string;
}) {
  return (
    <Popover>
      <PopoverTrigger asChild>
        <button
          type="button"
          aria-label={label}
          className="text-inherit underline decoration-transparent underline-offset-2 hover:decoration-current focus-visible:decoration-current"
        >
          {trigger}
        </button>
      </PopoverTrigger>
      <PopoverContent align="start" className="w-[min(22rem,calc(100vw-2rem))]">
        <p className="type-eyebrow text-text-3">{heading}</p>
        {name ? <p className="type-body mt-1 text-text-1">{name}</p> : null}
        <CopyableAddress address={address} />
      </PopoverContent>
    </Popover>
  );
}

export function CopyableAddress({ address }: { address: string }) {
  const valueRef = useRef<HTMLSpanElement>(null);
  const copy = async () => {
    try {
      if (!navigator.clipboard?.writeText) throw new Error("Clipboard unavailable");
      await navigator.clipboard.writeText(address);
      toast.success("Address copied");
    } catch {
      if (valueRef.current) {
        const range = document.createRange();
        range.selectNodeContents(valueRef.current);
        const selection = window.getSelection();
        selection?.removeAllRanges();
        selection?.addRange(range);
      }
      toast.error("Couldn't copy; address selected. Press Ctrl+C or ⌘C.");
    }
  };

  return (
    <div className="mt-2 flex min-w-0 items-center gap-2">
      <span
        ref={valueRef}
        data-testid="address-value"
        className="min-w-0 select-text break-all font-mono text-sm text-text-2"
      >
        {address}
      </span>
      <button
        type="button"
        aria-label={`Copy ${address}`}
        onClick={() => void copy()}
        className="inline-flex size-8 shrink-0 items-center justify-center rounded-sm text-text-3 hover:bg-muted hover:text-text-1 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
      >
        <Copy className="size-3.5" aria-hidden />
      </button>
    </div>
  );
}
