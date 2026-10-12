/* eslint-disable no-await-in-loop -- Sequential offsets and draft revision CAS require ordered upload requests. */
/* Bounded uploads persist immutable daemon resources for this compose session. */

import { useState, type Dispatch, type MutableRefObject, type SetStateAction } from "react";
import { toast } from "sonner";

import { supportsDraftAttachmentUpload, uploadComposeAttachment } from "../api";
import { errorMessage, type ComposeDraftState } from "./composeDraft";

export interface ComposeUploadProgress {
  /** Stable render key — duplicate filenames are legal within a batch. */
  id: string;
  name: string;
  done: boolean;
}

interface ComposeAttachmentsInput {
  draftRef: MutableRefObject<ComposeDraftState | null>;
  setDraft: Dispatch<SetStateAction<ComposeDraftState | null>>;
  setDirty: Dispatch<SetStateAction<boolean>>;
}

export function useComposeAttachments({ draftRef, setDraft, setDirty }: ComposeAttachmentsInput) {
  const [uploading, setUploading] = useState(0);
  const [uploadProgress, setUploadProgress] = useState<ComposeUploadProgress[]>([]);

  async function addFiles(files: FileList | File[]) {
    const current = draftRef.current;
    if (!current) return;
    const fileList = Array.from(files);
    if (fileList.length === 0) return;
    if (fileList.some((file) => file.size > 25 * 1024 * 1024)) {
      toast.error("Attachments must be within the daemon's 25 MiB resource limit");
      return;
    }
    if (!current.draftId) {
      toast.error("Save this draft before uploading attachments");
      return;
    }
    setUploading((value) => value + fileList.length);
    const batch = fileList.map((file) => ({
      id: crypto.randomUUID(),
      file,
    }));
    setUploadProgress((items) => [
      ...items,
      ...batch.map((entry) => ({ id: entry.id, name: entry.file.name, done: false })),
    ]);
    try {
      if (!(await supportsDraftAttachmentUpload()))
        throw new Error("Upgrade the daemon before uploading portable draft attachments");
      const paths: string[] = [];
      let revision = current.revision;
      for (const { id, file } of batch) {
        const bytes = await file.arrayBuffer();
        const digest = await crypto.subtle.digest("SHA-256", bytes);
        const sha256 = Array.from(new Uint8Array(digest), (byte) =>
          byte.toString(16).padStart(2, "0"),
        ).join("");
        const resourceId = crypto.randomUUID();
        const declared = await uploadComposeAttachment({
          draftPath: current.draftPath,
          accountId: current.accountId,
          operation: "begin",
          metadata: {
            id: resourceId,
            draft_id: current.draftId,
            account_id: current.accountId,
            filename: file.name,
            mime_type: file.type || "application/octet-stream",
            length_bytes: bytes.byteLength,
            sha256,
          },
        });
        revision = declared.revision;
        for (let offset = 0; offset < bytes.byteLength; offset += 256 * 1024) {
          const chunk = new Uint8Array(bytes.slice(offset, offset + 256 * 1024));
          let binary = "";
          for (const byte of chunk) binary += String.fromCharCode(byte);
          await uploadComposeAttachment({
            draftPath: current.draftPath,
            accountId: current.accountId,
            operation: "chunk",
            resourceId,
            offset,
            contentBase64: btoa(binary),
          });
        }
        const uploaded = await uploadComposeAttachment({
          draftPath: current.draftPath,
          accountId: current.accountId,
          operation: "finalize",
          resourceId,
          expectedRevision: revision,
        });
        revision = uploaded.revision;
        paths.push(uploaded.path);
        setUploadProgress((items) =>
          items.map((item) => (item.id === id ? { ...item, done: true } : item)),
        );
      }
      if (draftRef.current?.draftPath !== current.draftPath) return;
      setDraft((latest) =>
        latest && latest.draftPath === current.draftPath
          ? {
              ...latest,
              revision,
              frontmatter: {
                ...latest.frontmatter,
                attach: [...latest.frontmatter.attach, ...paths],
              },
            }
          : latest,
      );
      setDirty(true);
      toast.success(`Attached ${fileList.length} ${fileList.length === 1 ? "file" : "files"}`);
    } catch (error) {
      toast.error("Attachment failed", { description: errorMessage(error) });
    } finally {
      setUploading((value) => Math.max(0, value - fileList.length));
      // Drop this batch's entries; another concurrent batch keeps its own.
      const batchIds = new Set<string>(batch.map((entry) => entry.id));
      setUploadProgress((items) => items.filter((item) => !batchIds.has(item.id)));
    }
  }

  return { uploading, uploadProgress, addFiles };
}
