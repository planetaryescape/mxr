import { Trash2 } from "lucide-react";
import { useState, type FormEvent } from "react";
import { toast } from "sonner";

import { SettingRow, ToggleSetting } from "./settingsParts";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { useUiPrefs } from "@/state/uiPrefsStore";

export function NotificationsSection() {
  const notificationsEnabled = useUiPrefs((state) => state.notificationsEnabled);
  const notifyAllNewMail = useUiPrefs((state) => state.notifyAllNewMail);
  const vipAllowlist = useUiPrefs((state) => state.vipAllowlist);
  const setNotificationsEnabled = useUiPrefs((state) => state.setNotificationsEnabled);
  const setNotifyAllNewMail = useUiPrefs((state) => state.setNotifyAllNewMail);
  const addVip = useUiPrefs((state) => state.addVip);
  const removeVip = useUiPrefs((state) => state.removeVip);
  const [vip, setVip] = useState("");

  const enable = async (checked: boolean) => {
    if (checked && typeof Notification !== "undefined") {
      const permission =
        Notification.permission === "default"
          ? await Notification.requestPermission()
          : Notification.permission;
      if (permission === "denied") {
        toast.error("Notifications are blocked", {
          description: "Allow notifications for this site in the browser, then try again.",
        });
        return;
      }
    }
    setNotificationsEnabled(checked);
  };

  const submit = (event: FormEvent) => {
    event.preventDefault();
    if (!vip.trim()) return;
    addVip(vip.trim());
    setVip("");
  };

  return (
    <div>
      <ToggleSetting
        id="notifications-enabled"
        label="Browser notifications"
        description="Show a system notification when mail from a VIP arrives."
        checked={notificationsEnabled}
        onChange={(checked) => void enable(checked)}
      />
      <ToggleSetting
        id="notifications-all"
        label="Notify for all new mail"
        description="Not just VIPs."
        checked={notifyAllNewMail}
        onChange={setNotifyAllNewMail}
      />
      <SettingRow label="VIPs" description="Addresses or whole domains (@acme.com).">
        <form onSubmit={submit} className="flex gap-2">
          <Input
            aria-label="Add a VIP address or domain"
            value={vip}
            onChange={(event) => setVip(event.target.value)}
            placeholder="alice@example.com"
            className="h-8 w-56 text-xs"
          />
          <Button type="submit" size="sm" disabled={!vip.trim()}>
            Add
          </Button>
        </form>
      </SettingRow>
      {vipAllowlist.length > 0 ? (
        <ul aria-label="VIPs">
          {vipAllowlist.map((item) => (
            <li
              key={item}
              className="flex items-center justify-between gap-3 border-b border-border/60 py-1.5 pl-3"
            >
              <span className="truncate font-mono text-xs">{item}</span>
              <Button
                variant="ghost"
                size="icon-xs"
                aria-label={`Remove VIP ${item}`}
                onClick={() => removeVip(item)}
              >
                <Trash2 />
              </Button>
            </li>
          ))}
        </ul>
      ) : null}
    </div>
  );
}
