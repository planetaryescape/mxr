const ROUTE_TITLES: Record<string, string> = {
  accounts: "Accounts",
  analytics: "Analytics",
  arrivals: "Arrivals",
  archive: "Archive",
  "paper-trail": "Paper trail",
  deliveries: "Deliveries",
  desk: "Desk",
  focus: "Focus",
  compose: "Compose",
  diagnostics: "Diagnostics",
  dev: "Development tools",
  drafts: "Drafts",
  find: "Find",
  invites: "Calendar invites",
  messages: "Messages",
  owed: "Owed replies",
  m: "Mail",
  now: "Now",
  onboarding: "Welcome",
  reading: "Reading",
  rules: "Rules",
  reply: "Reply",
  "reply-queue": "Reply queue",
  screener: "Screener",
  search: "Search",
  settings: "Settings",
  snoozed: "Snoozed",
  updates: "Updates",
  todo: "To do",
  sender: "Sender",
  subscriptions: "Subscriptions",
  activity: "Activity",
  jobs: "Jobs",
};

const SETTINGS_TITLES: Record<string, string> = {
  about: "About",
  accounts: "Accounts",
  compose: "Compose settings",
  keybindings: "Keyboard settings",
  llm: "Language model settings",
  notifications: "Notifications settings",
  reader: "Reader settings",
  snippets: "Snippets settings",
  sound: "Sound settings",
  theme: "Appearance settings",
  token: "Bridge token settings",
  voice: "Voice settings",
};

/** Titles stay at the page level so mail subjects and sender names stay private. */
export function routeTitleForPath(pathname: string): string {
  const [section, subsection] = pathname.split("/").filter(Boolean);
  if (!section) return "Now";
  if (section === "settings") {
    const title = SETTINGS_TITLES[subsection ?? ""];
    return Object.hasOwn(SETTINGS_TITLES, subsection ?? "") && typeof title === "string"
      ? title
      : "Settings";
  }
  const title = ROUTE_TITLES[section];
  return Object.hasOwn(ROUTE_TITLES, section) && typeof title === "string" ? title : "mxr";
}
