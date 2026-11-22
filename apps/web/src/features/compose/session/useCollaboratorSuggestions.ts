import { useEffect, useRef, useState } from "react";

import { suggestComposeCollaborators, type SuggestedCollaborator } from "../api";
import { splitAddresses, type ComposeDraftState } from "./composeDraft";

export function useCollaboratorSuggestions(
  draft: ComposeDraftState | null,
): SuggestedCollaborator[] {
  const [collaboratorSuggestions, setCollaboratorSuggestions] = useState<SuggestedCollaborator[]>(
    [],
  );
  const collaboratorsFetchedRef = useRef(new Set<string>());

  const collaboratorDraftPath = draft?.draftPath;
  const collaboratorAccountId = draft?.accountId;
  const collaboratorTo = draft?.frontmatter.to ?? "";
  useEffect(() => {
    if (!collaboratorDraftPath || !collaboratorAccountId) return;
    if (collaboratorsFetchedRef.current.has(collaboratorDraftPath)) return;
    if (splitAddresses(collaboratorTo).length === 0) return;
    const handle = window.setTimeout(() => {
      collaboratorsFetchedRef.current.add(collaboratorDraftPath);
      suggestComposeCollaborators(collaboratorDraftPath, collaboratorAccountId)
        .then((response) => setCollaboratorSuggestions(response.suggestions ?? []))
        .catch(() => {});
    }, 1000);
    return () => window.clearTimeout(handle);
  }, [collaboratorDraftPath, collaboratorAccountId, collaboratorTo]);

  return draft
    ? collaboratorSuggestions.filter(
        (item) =>
          !`${draft.frontmatter.to},${draft.frontmatter.cc},${draft.frontmatter.bcc}`
            .toLowerCase()
            .includes(item.email.toLowerCase()),
      )
    : [];
}
