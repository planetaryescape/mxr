/*
 * Browser file uploads for a compose session. Each file is uploaded to the
 * bridge, which returns a local temp path that is written into the draft's
 * `attach` frontmatter.
 */

import { useState, type Dispatch, type MutableRefObject, type SetStateAction } from "react";
import { toast } from "sonner";

import { uploadComposeAttachment } from "../api";
import { errorMessage, type ComposeDraftState } from "./composeDraft";

const LARGE_ATTACHMENT_BYTES = 10 * 1024 * 1024;

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
    // Warn (but still upload) on oversized files — many receiving servers
    // bounce attachments past ~10 MB.
    for (const file of fileList) {
      if (file.size > LARGE_ATTACHMENT_BYTES) {
        toast.warning(`${file.name} is ${formatMegabytes(file.size)}`, {
          description: "Large attachments are often rejected by mail servers. Uploading anyway.",
        });
      }
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
      const paths = await Promise.all(
        batch.map(async ({ id, file }) => {
          const contentBase64 = await fileToBase64(file);
          const uploaded = await uploadComposeAttachment({
            draftPath: current.draftPath,
            filename: file.name,
            contentBase64,
          });
          setUploadProgress((items) =>
            items.map((item) => (item.id === id ? { ...item, done: true } : item)),
          );
          return uploaded.path;
        }),
      );
      setDraft((latest) =>
        latest
          ? {
              ...latest,
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

function fileToBase64(file: File): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.addEventListener(
      "load",
      () => {
        const value = String(reader.result ?? "");
        resolve(value.includes(",") ? value.slice(value.indexOf(",") + 1) : value);
      },
      { once: true },
    );
    reader.addEventListener(
      "error",
      () => reject(reader.error ?? new Error("Failed to read file")),
      { once: true },
    );
    reader.readAsDataURL(file);
  });
}

function formatMegabytes(bytes: number): string {
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}
