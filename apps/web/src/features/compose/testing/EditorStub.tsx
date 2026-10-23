/*
 * A stand-in for the editors in the compose tests: a controlled textarea that
 * takes focus when asked, which are the two things the compose surface relies
 * on. The real editors need a layout engine jsdom does not have.
 */

import { useEffect, useRef } from "react";

export interface EditorStubProps {
  value: string;
  onChange: (value: string) => void;
  autoFocus?: boolean;
  focusNonce?: number;
}

export function EditorStub({
  value,
  onChange,
  autoFocus = false,
  focusNonce = 0,
}: EditorStubProps) {
  const ref = useRef<HTMLTextAreaElement>(null);
  useEffect(() => {
    if (autoFocus || focusNonce > 0) ref.current?.focus();
  }, [autoFocus, focusNonce]);
  return (
    <textarea
      ref={ref}
      aria-label="Message body"
      value={value}
      onChange={(event) => onChange(event.target.value)}
    />
  );
}
