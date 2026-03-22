import type { MailboxRow } from "../../shared/types";

export type FlattenedEntry =
  | {
      kind: "header";
      id: string;
      label: string;
    }
  | {
      kind: "row";
      id: string;
      row: MailboxRow;
    };
