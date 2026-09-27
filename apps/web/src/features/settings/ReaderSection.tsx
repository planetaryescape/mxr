/*
 * How mail opens and reads: layout, the default body view, list grouping,
 * HTML canvas, and the senders whose remote images always load.
 */

import { X } from "lucide-react";

import { DENSITIES } from "./AppearanceSection";
import { SelectSetting, SettingRow } from "./settingsParts";
import { Button } from "@/components/ui/button";
import {
  useUiPrefs,
  type EmailHtmlTheme,
  type ReaderLayout,
  type ReaderView,
} from "@/state/uiPrefsStore";

export function ReaderSection() {
  const prefs = useUiPrefs();
  return (
    <div>
      <SelectSetting<ReaderLayout>
        label="Layout"
        description="Split keeps the list beside the conversation; full gives the conversation the page."
        value={prefs.readerLayout}
        options={[
          { value: "split", label: "Split" },
          { value: "full", label: "Full page" },
        ]}
        onChange={prefs.setReaderLayout}
      />
      <SelectSetting<ReaderView>
        label="Open messages as"
        description="Formatted is sanitized HTML. Reader strips it to clean text. Plain shows the text part."
        value={prefs.readerView}
        options={[
          { value: "formatted", label: "Formatted" },
          { value: "reader", label: "Reader" },
          { value: "plain", label: "Plain text" },
        ]}
        onChange={prefs.setReaderView}
      />
      <SelectSetting<"threads" | "messages">
        label="List shows"
        description="Group mail into conversations, or list every message on its own."
        value={prefs.listMode}
        options={[
          { value: "threads", label: "Conversations" },
          { value: "messages", label: "Messages" },
        ]}
        onChange={prefs.setListMode}
      />
      <SelectSetting
        label="Density"
        description="Row height in mail lists."
        value={prefs.density}
        options={DENSITIES}
        onChange={prefs.setDensity}
      />
      <SelectSetting<EmailHtmlTheme>
        label="HTML mail canvas"
        description="Dark renders HTML mail on a dark canvas; original keeps the sender's colours."
        value={prefs.emailHtmlTheme}
        options={[
          { value: "dark", label: "Dark" },
          { value: "original", label: "Original" },
        ]}
        onChange={prefs.setEmailHtmlTheme}
      />
      <SettingRow
        label="Always load images from"
        description="Remote images are blocked until you allow them. Senders you always allow are listed here."
      >
        <span className="font-mono text-2xs text-muted-foreground">
          {prefs.remoteImageSenders.length === 0
            ? "none yet"
            : `${prefs.remoteImageSenders.length} allowed`}
        </span>
      </SettingRow>
      {prefs.remoteImageSenders.length > 0 ? (
        <ul aria-label="Senders whose images always load">
          {prefs.remoteImageSenders.map((sender) => (
            <li
              key={sender}
              className="flex items-center justify-between gap-3 border-b border-border/60 py-1.5 pl-3"
            >
              <span className="truncate font-mono text-xs">{sender}</span>
              <Button
                variant="ghost"
                size="icon-xs"
                aria-label={`Stop loading images from ${sender}`}
                onClick={() => prefs.forgetRemoteImagesFrom(sender)}
              >
                <X />
              </Button>
            </li>
          ))}
        </ul>
      ) : null}
    </div>
  );
}
