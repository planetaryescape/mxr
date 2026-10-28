/*
 * A stand-in for the editors in the compose tests: a controlled textarea that
 * takes focus when asked, and reports its selection the way the Markdown
 * editor does (null when empty). The real editors need a layout engine jsdom
 * does not have.
 */

import { useEffect, useRef } from "react";

export interface EditorStubProps {
  value: string;
  onChange: (value: string) => void;
  autoFocus?: boolean;
  focusNonce?: number;
  readOnly?: boolean;
  onSelectionChange?: (selection: { from: number; to: number } | null) => void;
}

export function EditorStub({
  value,
  onChange,
  autoFocus = false,
  focusNonce = 0,
  readOnly = false,
  onSelectionChange,
}: EditorStubProps) {
  const ref = useRef<HTMLTextAreaElement>(null);
  useEffect(() => {
    if (autoFocus || focusNonce > 0) ref.current?.focus();
  }, [autoFocus, focusNonce]);
  return (
    <textarea
      ref={ref}
      aria-label="Message body"
      readOnly={readOnly}
      value={value}
      onChange={(event) => onChange(event.target.value)}
      onSelect={(event) => {
        const { selectionStart, selectionEnd } = event.currentTarget;
        onSelectionChange?.(
          selectionStart === selectionEnd ? null : { from: selectionStart, to: selectionEnd },
        );
      }}
    />
  );
}
