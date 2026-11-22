const { mkdtempSync, readFileSync, readdirSync, rmSync } = require("node:fs");
const { tmpdir } = require("node:os");
const { join } = require("node:path");
const { after, afterEach, beforeEach, test } = require("node:test");
const assert = require("node:assert/strict");
const Module = require("node:module");
const React = require("react");
const ts = require("typescript");

const dataDirectory = mkdtempSync(join(tmpdir(), "mxr-reading-pane-tests-"));
after(() => rmSync(dataDirectory, { recursive: true, force: true }));

for (const extension of [".ts", ".tsx"]) {
  require.extensions[extension] = (loaded, filename) => {
    const { outputText } = ts.transpileModule(readFileSync(filename, "utf8"), {
      compilerOptions: {
        module: ts.ModuleKind.CommonJS,
        target: ts.ScriptTarget.ES2022,
        esModuleInterop: true,
        jsx: ts.JsxEmit.ReactJSX,
      },
      fileName: filename,
    });
    loaded._compile(outputText, filename);
  };
}

function host(name, statics = {}) {
  const component = (props) => React.createElement(name, props);
  return Object.assign(component, statics);
}

const metadata = (prefix) =>
  host(`${prefix}.Metadata`, {
    Label: host(`${prefix}.Metadata.Label`),
    Separator: host(`${prefix}.Metadata.Separator`),
    TagList: host(`${prefix}.Metadata.TagList`, { Item: host(`${prefix}.Metadata.TagList.Item`) }),
  });

// A row's `detail` and `actions` are props, but Raycast mounts them.
const ListItem = ({ detail, actions, ...props }) =>
  React.createElement("List.Item", props, detail, actions);
ListItem.Detail = host("List.Item.Detail", { Metadata: metadata("List.Item.Detail") });
const DetailView = ({ metadata: panel, actions, ...props }) =>
  React.createElement("Detail", props, panel, actions);
DetailView.Metadata = metadata("Detail");

const cacheStore = new Map();
class Cache {
  get(key) {
    return cacheStore.get(key);
  }
  set(key, value) {
    cacheStore.set(key, value);
  }
  has(key) {
    return cacheStore.has(key);
  }
  remove(key) {
    return cacheStore.delete(key);
  }
  subscribe() {
    return () => undefined;
  }
}

const names = new Proxy({}, { get: (_, name) => String(name) });
const raycast = {
  Action: host("Action", {
    Push: host("Action.Push"),
    OpenInBrowser: host("Action.OpenInBrowser"),
    CopyToClipboard: host("Action.CopyToClipboard"),
  }),
  ActionPanel: host("ActionPanel", {
    Section: host("ActionPanel.Section"),
    Submenu: host("ActionPanel.Submenu"),
  }),
  Alert: { ActionStyle: names },
  Cache,
  Color: names,
  Detail: DetailView,
  Form: host("Form"),
  Icon: names,
  LaunchType: names,
  List: host("List", {
    Dropdown: host("List.Dropdown", {
      Item: host("List.Dropdown.Item"),
      Section: host("List.Dropdown.Section"),
    }),
    EmptyView: host("List.EmptyView"),
    Item: ListItem,
    Section: host("List.Section"),
  }),
  Toast: { Style: names },
  confirmAlert: async () => false,
  environment: { appearance: "dark", launchType: "userInitiated" },
  getPreferenceValues: () => ({ dataDirectory }),
  showToast: async () => ({ hide: async () => undefined }),
};

const loadModule = Module._load;
Module._load = function (request, parent, isMain) {
  if (request === "@raycast/api") return raycast;
  return loadModule.call(this, request, parent, isMain);
};

const consoleError = console.error;
console.error = (...args) => {
  if (!String(args[0]).includes("react-test-renderer is deprecated")) consoleError(...args);
};
globalThis.IS_REACT_ACT_ENVIRONMENT = true;
const { act, create } = require("react-test-renderer");

const { default: Inbox } = require("../src/inbox.tsx");
const { default: Messages } = require("../src/messages.tsx");
const { default: Updates } = require("../src/updates.tsx");
const { default: Reading } = require("../src/reading.tsx");
const { ThreadDetail } = require("../src/components/ThreadDetail.tsx");
const { senderFace } = require("../src/components/faces.ts");
const { fileSize } = require("../src/components/threadFacts.ts");
const { keyFor } = require("../src/components/keys.ts");

const PAST_READ_DELAY_MS = 800;
const when = "2026-11-10T09:00:00Z";

const thread = {
  thread: { id: "t1", account_id: "a1", subject: "Quarterly report", message_count: 1 },
  messages: [
    {
      id: "m1",
      account_id: "a1",
      thread_id: "t1",
      sender: "Ada Lovelace",
      sender_detail: "ada@example.test",
      subject: "Quarterly report",
      snippet: "The numbers",
      date: when,
      unread: true,
      starred: false,
      has_attachments: true,
      to: [{ name: "BK", email: "bk@example.test" }],
      labels: [
        { id: "l1", name: "INBOX", kind: "system", color: null },
        { id: "l2", name: "Finance", kind: "user", color: "#fb4c2f" },
        { id: "l3", name: "Clients", kind: "folder", color: "not-a-colour" },
      ],
    },
  ],
  bodies: [
    {
      message_id: "m1",
      text_plain: "The body of the newest message.",
      attachments: [
        { id: "f1", filename: "report.pdf", size_bytes: 86016, content_id: null },
        { id: "f2", filename: "logo.png", size_bytes: 2048, content_id: "logo@mail" },
      ],
    },
  ],
};

const mailRow = { ...thread.messages[0], message_count: 1 };
const personRow = {
  id: "person:ada@example.test",
  account_id: "a1",
  kind: "person",
  title: "Ada Lovelace",
  person: { id: "ada@example.test", name: "Ada Lovelace", addresses: ["ada@example.test"] },
  topics: [{ thread_id: "t1", subject: "Quarterly report", reply_to_message_id: "m1" }],
  last_at: when,
  unread: true,
  overdue: true,
  why: "Here because she asked you a question.",
};
const updateLine = {
  id: "u1",
  account_id: "a1",
  source_name: "Build bot",
  sender_email: "bot@example.test",
  fact: "Run failed",
  count: 1,
  latest_at: when,
  latest_thread_id: "t1",
  thread_ids: ["t1"],
  why: "Here because it is automated.",
};
const readingItem = {
  item_key: "m1:0",
  account_id: "a1",
  thread_id: "t1",
  title: "An essay",
  source: "The Letter",
  sender_email: "letter@example.test",
  minutes: 4,
  progress: 0.5,
  on_later: false,
  why: "Here because you subscribed.",
  arrived_at: when,
};

const routes = {
  "/api/v1/auth/local-token": { token: "test-token" },
  "/api/v1/platform/accounts": {
    accounts: [
      { account_id: "a1", key: "work", email: "me@work.test", name: "Work", enabled: true },
    ],
  },
  "/api/v1/mail/mailbox": { mailbox: { groups: [{ rows: [mailRow] }], counts: { total: 1 } } },
  "/api/v1/mail/people": {
    messages: {
      your_turn: [personRow],
      pinned: [],
      recent: [],
      quiet: [],
      recent_total: 0,
      quiet_total: 0,
      row_count: 1,
    },
  },
  "/api/v1/mail/updates": { digest: { needs_a_look: [updateLine], changed: [], routine: [] } },
  "/api/v1/mail/reading": {
    edition: {
      bands: [{ band: "earlier", label: "Earlier", items: [readingItem] }],
      later: [],
      later_count: 0,
    },
  },
  "/api/v1/mail/threads/t1": thread,
  "/api/v1/mail/codes": { codes: [] },
  "/api/v1/mail/mutations/read": {},
};

let requests;
const originalFetch = global.fetch;
const mounted = [];
beforeEach(() => {
  requests = [];
  cacheStore.clear();
  global.fetch = async (input, init = {}) => {
    const { pathname } = new URL(input);
    requests.push({ method: init.method ?? "GET", path: pathname, body: init.body });
    if (!(pathname in routes)) throw new Error(`Unexpected bridge request: ${pathname}`);
    return Response.json(routes[pathname]);
  };
});
afterEach(async () => {
  await act(async () => mounted.splice(0).forEach((view) => view.unmount()));
  global.fetch = originalFetch;
});

const settle = (ms = 50) => act(() => new Promise((resolve) => setTimeout(resolve, ms)));

async function mount(element) {
  let view;
  await act(async () => {
    view = create(element);
  });
  mounted.push(view);
  await settle();
  return view;
}

const paneAction = (view) =>
  view.root.find((node) => node.type === "Action" && /Reading Pane$/.test(node.props.title ?? ""));
const readRequests = () => requests.filter(({ path }) => path.endsWith("/mutations/read"));
const threadRequests = () => requests.filter(({ path }) => path === "/api/v1/mail/threads/t1");

for (const [name, Command, rowId] of [
  ["Inbox", Inbox, "t1"],
  ["Messages", Messages, "turn-a1-person:ada@example.test"],
  ["Updates", Updates, "u1"],
  ["Reading", Reading, "earlier-m1:0"],
]) {
  test(`${name}: the pane shows the newest message and leaves it unread`, async () => {
    const view = await mount(React.createElement(Command));
    const list = () => view.root.findByType("List");
    await act(async () => list().props.onSelectionChange(rowId));
    await settle();
    assert.equal(threadRequests().length, 0, "a hidden pane fetches nothing");

    await act(async () => paneAction(view).props.onAction());
    await settle(PAST_READ_DELAY_MS);

    assert.equal(list().props.isShowingDetail, true);
    const pane = view.root.findByType("List.Item.Detail");
    assert.match(pane.props.markdown, /The body of the newest message\./);
    assert.ok(threadRequests().every(({ method }) => method === "GET"));
    assert.ok(threadRequests().length > 0, "the selected row's conversation is fetched");
    assert.deepEqual(readRequests(), [], "showing the pane must not mark mail read");
  });
}

test("opening the reader does mark the conversation read", async () => {
  await mount(React.createElement(ThreadDetail, { threadId: "t1", webPath: "/m/inbox/t1" }));
  await settle(PAST_READ_DELAY_MS);
  assert.equal(readRequests().length, 1);
  assert.deepEqual(JSON.parse(readRequests()[0].body), { message_ids: ["m1"], read: true });
});

test("the reader's side panel names the account, labels and attachment sizes", async () => {
  const view = await mount(
    React.createElement(ThreadDetail, { threadId: "t1", webPath: "/m/inbox/t1" }),
  );
  const tags = (title) =>
    view.root
      .findAll((node) => node.type === "Detail.Metadata.TagList" && node.props.title === title)
      .flatMap((list) => list.findAllByType("Detail.Metadata.TagList.Item"))
      .map((item) => item.props);

  assert.deepEqual(
    tags("Account").map((tag) => tag.text),
    ["work"],
  );
  assert.deepEqual(
    tags("Labels").map(({ text, color }) => [text, color]),
    [
      ["Finance", "#fb4c2f"],
      ["Clients", undefined],
    ],
  );
  assert.deepEqual(
    view.root.findAllByType("Detail.Metadata.Label").map((label) => label.props.text),
    ["report.pdf · 84 KB"],
  );
});

test("rows show initials, an overdue tag and reading progress", async () => {
  const messages = await mount(React.createElement(Messages));
  const person = messages.root.findByType("List.Item");
  assert.match(person.props.icon, /^data:image\/svg\+xml,/);
  assert.ok(person.props.accessories.some((entry) => entry.tag?.value === "Overdue"));

  const reading = await mount(React.createElement(Reading));
  const item = reading.root.findByType("List.Item");
  assert.ok(item.props.accessories.some((entry) => entry.tooltip === "50% read"));
});

test("initials are drawn locally and survive a hostile sender name", () => {
  global.fetch = async () => assert.fail("initials must not touch the network");
  for (const [name, email, expected] of [
    ["Ada Lovelace", "ada@example.test", "AL"],
    ["<b>&amp;", "x@example.test", "BA"],
    ["&", "ada@example.test", "A"],
    ["<>", null, "?"],
    [null, "no-reply@example.test", "NR"],
    ["sales@example.test", null, "S"],
  ]) {
    const face = senderFace(name, email);
    assert.match(face, /^data:image\/svg\+xml,/);
    const svg = decodeURIComponent(face.slice("data:image/svg+xml,".length));
    assert.equal(svg.match(/<text[^>]*>([^<&]*)<\/text>/)?.[1], expected, String(name));
  }
});

test("attachment sizes read in the unit that fits", () => {
  assert.equal(fileSize(512), "512 B");
  assert.equal(fileSize(86016), "84 KB");
  assert.equal(fileSize(1048575), "1.0 MB");
  assert.equal(fileSize(5 * 1024 * 1024), "5.0 MB");
});

test("no other Raycast action uses the pane's key", () => {
  assert.deepEqual(keyFor("raycast.reading-pane"), { modifiers: ["cmd"], key: "d" });
  const source = join(__dirname, "../src");
  const files = readdirSync(source, { recursive: true })
    .filter((file) => /\.tsx?$/.test(file))
    .map((file) => readFileSync(join(source, file), "utf8"));
  const ids = new Set(
    files.flatMap((text) => [...text.matchAll(/keyFor\("([^"]+)"\)/g)].map((m) => m[1])),
  );
  const taken = [...ids].filter((id) => {
    const { modifiers, key } = keyFor(id);
    return key === "d" && modifiers.join() === "cmd";
  });
  assert.deepEqual(taken, ["raycast.reading-pane"]);
  assert.ok(!files.some((text) => /key: "d"/.test(text)), "no hard-coded key uses d");
});
