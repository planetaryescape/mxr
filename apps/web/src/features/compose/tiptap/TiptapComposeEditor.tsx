import Placeholder from "@tiptap/extension-placeholder";
import { EditorContent, useEditor, useEditorState, type Editor } from "@tiptap/react";
import StarterKit from "@tiptap/starter-kit";
import { Bold, Italic, Link2, List, ListOrdered, RemoveFormatting } from "lucide-react";
import { useEffect } from "react";

import { Button } from "@/components/ui/button";

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
          "mx-auto h-full min-h-full w-full max-w-[720px] px-6 py-6 pb-14 text-[15px] leading-[1.65] outline-none",
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
        if ((event.metaKey || event.ctrlKey) && event.key === "Enter" && !event.shiftKey) {
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

  return (
    <div className="flex h-full min-h-0 flex-col">
      <TiptapToolbar editor={editor} />
      <EditorContent editor={editor} className="min-h-0 flex-1 overflow-auto" />
    </div>
  );
}

function TiptapToolbar({ editor }: { editor: Editor | null }) {
  const active = useEditorState({
    editor,
    selector: ({ editor: instance }) =>
      instance
        ? {
            bold: instance.isActive("bold"),
            italic: instance.isActive("italic"),
            bulletList: instance.isActive("bulletList"),
            orderedList: instance.isActive("orderedList"),
            link: instance.isActive("link"),
          }
        : null,
  });
  if (!editor) return null;

  function setLink() {
    if (!editor) return;
    const previous = (editor.getAttributes("link").href as string | undefined) ?? "";
    const url = window.prompt("Link URL", previous);
    if (url === null) return;
    if (!url.trim()) {
      editor.chain().focus().extendMarkRange("link").unsetLink().run();
      return;
    }
    editor.chain().focus().extendMarkRange("link").setLink({ href: url.trim() }).run();
  }

  return (
    <div
      role="toolbar"
      aria-label="Text formatting"
      className="mx-auto flex w-full max-w-[720px] shrink-0 items-center gap-0.5 border-b border-border/60 px-4 py-1"
    >
      <ToolbarButton
        label="Bold"
        active={active?.bold}
        onClick={() => editor.chain().focus().toggleBold().run()}
      >
        <Bold className="size-3.5" />
      </ToolbarButton>
      <ToolbarButton
        label="Italic"
        active={active?.italic}
        onClick={() => editor.chain().focus().toggleItalic().run()}
      >
        <Italic className="size-3.5" />
      </ToolbarButton>
      <ToolbarButton
        label="Bullet list"
        active={active?.bulletList}
        onClick={() => editor.chain().focus().toggleBulletList().run()}
      >
        <List className="size-3.5" />
      </ToolbarButton>
      <ToolbarButton
        label="Ordered list"
        active={active?.orderedList}
        onClick={() => editor.chain().focus().toggleOrderedList().run()}
      >
        <ListOrdered className="size-3.5" />
      </ToolbarButton>
      <ToolbarButton label="Link" active={active?.link} onClick={setLink}>
        <Link2 className="size-3.5" />
      </ToolbarButton>
      <ToolbarButton
        label="Clear formatting"
        onClick={() => editor.chain().focus().clearNodes().unsetAllMarks().run()}
      >
        <RemoveFormatting className="size-3.5" />
      </ToolbarButton>
    </div>
  );
}

function ToolbarButton({
  label,
  active,
  onClick,
  children,
}: {
  label: string;
  active?: boolean;
  onClick: () => void;
  children: React.ReactNode;
}) {
  return (
    <Button
      type="button"
      variant={active ? "secondary" : "ghost"}
      size="icon-xs"
      aria-label={label}
      aria-pressed={active ?? false}
      title={label}
      // Keep the selection in the editor — formatting acts on it.
      onMouseDown={(event) => event.preventDefault()}
      onClick={onClick}
    >
      {children}
    </Button>
  );
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
