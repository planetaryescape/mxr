import Placeholder from "@tiptap/extension-placeholder";
import { EditorContent, useEditor } from "@tiptap/react";
import StarterKit from "@tiptap/starter-kit";
import { useEffect } from "react";

interface TiptapComposeEditorProps {
  value: string;
  onChange: (value: string) => void;
  onSave: () => void;
  onSend: () => void;
  onDiscard: () => void;
  autoFocus?: boolean;
}

export function TiptapComposeEditor({
  value,
  onChange,
  onSave,
  onSend,
  onDiscard,
  autoFocus = false,
}: TiptapComposeEditorProps) {
  const editor = useEditor({
    extensions: [StarterKit, Placeholder.configure({ placeholder: "Write the message body..." })],
    content: textToDoc(value),
    editorProps: {
      attributes: {
        class:
          "min-h-[520px] rounded-lg border border-border bg-surface px-4 py-3 text-sm leading-6 outline-none",
        role: "textbox",
        "aria-label": "Message body",
      },
      handleKeyDown: (_view, event) => {
        if ((event.metaKey || event.ctrlKey) && event.key === "Backspace") {
          event.preventDefault();
          onDiscard();
          return true;
        }
        if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "s") {
          event.preventDefault();
          onSave();
          return true;
        }
        if ((event.metaKey || event.ctrlKey) && event.key === "Enter") {
          event.preventDefault();
          onSend();
          return true;
        }
        return false;
      },
    },
    onUpdate: ({ editor: activeEditor }) =>
      onChange(activeEditor.state.doc.textBetween(0, activeEditor.state.doc.content.size, "\n\n")),
  });

  useEffect(() => {
    if (!editor) return;
    const current = editor.state.doc.textBetween(0, editor.state.doc.content.size, "\n\n");
    if (current !== value) {
      editor.commands.setContent(textToDoc(value), { emitUpdate: false });
    }
  }, [editor, value]);

  useEffect(() => {
    if (!editor || !autoFocus) return;
    window.setTimeout(() => editor.commands.focus("end"), 0);
  }, [autoFocus, editor]);

  return <EditorContent editor={editor} />;
}

function textToDoc(value: string) {
  const lines = value.split(/\n{2,}/);
  return {
    type: "doc",
    content: (lines.length > 0 ? lines : [""]).map((line) => ({
      type: "paragraph",
      content: line ? [{ type: "text", text: line }] : undefined,
    })),
  };
}
