import { defineConfig } from 'astro/config';
import starlight from '@astrojs/starlight';

export default defineConfig({
  site: 'https://mxr.sh',
  // Pages a later page replaced; old links still land.
  redirects: {
    '/guides/desk/': '/guides/now/',
    '/guides/no-native-desktop-app/': '/guides/desktop-app/',
  },
  integrations: [
    starlight({
      title: 'mxr',
      description: 'A local mail runtime for you and your agents. Work with synced mail history and shared local state.',
      disable404Route: true,
      social: [
        { icon: 'github', label: 'GitHub', href: 'https://github.com/planetaryescape/mxr' },
      ],
      customCss: [
        './src/styles/custom.css',
      ],
      expressiveCode: {
        // Expressive Code gives overflowing blocks an unnamed region. Wrapping
        // avoids that on phones while its copy button keeps the original text.
        defaultProps: {
          wrap: true,
          // Deep visual indentation can still force wrapped lines to overflow.
          preserveIndent: false,
        },
      },
      components: {
        Footer: './src/components/Footer.astro',
      },
      head: [
        { tag: 'meta', attrs: { name: 'theme-color', content: '#071522' } },
        // Social cards / SEO. Starlight derives canonical, og:title/type/url/locale/
        // description/site_name, and twitter:card from `site` + page frontmatter; these
        // add the image and the bits it does not emit. Absolute URLs are required for og.
        // Do not add twitter:title or twitter:description here: one site-wide value
        // overrides the per-page og:* fallback and labels every page the homepage.
        { tag: 'meta', attrs: { property: 'og:image', content: 'https://mxr.sh/og.png' } },
        { tag: 'meta', attrs: { property: 'og:image:width', content: '1200' } },
        { tag: 'meta', attrs: { property: 'og:image:height', content: '630' } },
        { tag: 'meta', attrs: { property: 'og:image:type', content: 'image/png' } },
        { tag: 'meta', attrs: { property: 'og:image:alt', content: 'mxr syncs connected mail into a local mailbox for people, scripts, and agents to use.' } },
        { tag: 'meta', attrs: { name: 'twitter:image', content: 'https://mxr.sh/og.png' } },
        { tag: 'meta', attrs: { name: 'twitter:image:alt', content: 'mxr syncs connected mail into a local mailbox for people, scripts, and agents to use.' } },
        { tag: 'link', attrs: { rel: 'apple-touch-icon', sizes: '180x180', href: '/apple-touch-icon.png' } },
        { tag: 'meta', attrs: { name: 'author', content: 'planetaryescape' } },
        { tag: 'meta', attrs: { name: 'keywords', content: 'email client, local-first email, CLI email, terminal email client, TUI email, Gmail CLI, Outlook CLI, IMAP client, SMTP, Rust, MCP server, agent email, offline email, self-hosted email' } },
      ],
      sidebar: [
        {
          label: 'Start here',
          items: [
            { label: 'Why mxr', slug: 'guides/why-mxr' },
            { label: 'Installation', slug: 'getting-started/install' },
            { label: 'Quick Start', slug: 'getting-started/quick-start' },
            { label: 'Use mxr with an Existing Agent', slug: 'getting-started/agent-quick-start' },
            { label: 'Examples', slug: 'examples' },
            { label: 'Videos', slug: 'videos' },
            { label: 'Gmail Setup', slug: 'getting-started/gmail-setup' },
            { label: 'IMAP / SMTP Setup', slug: 'getting-started/imap-smtp-setup' },
            { label: 'First Sync', slug: 'getting-started/first-sync' },
          ],
        },
        {
          label: 'Work with your mail',
          collapsed: true,
          items: [
            { label: 'Work through your day', slug: 'guides/your-day' },
            { label: 'Start from Now', slug: 'guides/now' },
            { label: 'Email modes', slug: 'guides/email-modes' },
            { label: 'Messages', slug: 'guides/messages' },
            { label: 'Focus and reply', slug: 'guides/focus-and-reply' },
            { label: 'To do', slug: 'guides/todo' },
            { label: 'Review later evidence', slug: 'guides/todo-resolution' },
            { label: 'Updates', slug: 'guides/updates' },
            { label: 'Reading', slug: 'guides/reading' },
            { label: 'Archive', slug: 'guides/archive' },
            { label: 'Marginalia', slug: 'guides/marginalia' },
            { label: 'Personal categories', slug: 'guides/personal-categories' },
          ],
        },
        {
          label: 'Search, understand and automate',
          collapsed: true,
          items: [
            { label: 'Search', slug: 'guides/search' },
            { label: 'Semantic search', slug: 'guides/semantic-search' },
            { label: 'Analytics', slug: 'guides/analytics' },
            { label: 'Sender view', slug: 'guides/sender-view' },
            { label: 'Archive intelligence', slug: 'guides/archive-intelligence' },
            { label: 'For agents', slug: 'guides/for-agents' },
            { label: 'Agent skill', slug: 'guides/agent-skill' },
            { label: 'Recipes', slug: 'guides/recipes' },
            { label: 'Rules', slug: 'guides/rules' },
            { label: 'Assistant', slug: 'guides/assistant' },
            { label: 'Forgotten work', slug: 'guides/forgotten-work' },
            { label: 'Automated follow-ups', slug: 'guides/automated-followups' },
            { label: 'Deliveries', slug: 'guides/deliveries' },
            { label: 'Timing and cadence', slug: 'guides/timing-and-cadence' },
            { label: 'Briefings and loop-in', slug: 'guides/briefings-and-loop-in' },
          ],
        },
        {
          label: 'Write and organise',
          collapsed: true,
          items: [
            { label: 'Compose', slug: 'guides/compose' },
            { label: 'Linked drafts', slug: 'guides/linked-drafts' },
            { label: 'Crash-safe drafts', slug: 'guides/crash-safe-drafts' },
            { label: 'Pre-send safety', slug: 'guides/pre-send-safety' },
            { label: 'Snippets', slug: 'guides/snippets' },
            { label: 'Mail merge', slug: 'guides/mail-merge' },
            { label: 'Calendar invites', slug: 'guides/calendar-invites' },
            { label: 'Labels and saved searches', slug: 'guides/labels-and-saved-searches' },
            { label: 'Triage flow', slug: 'guides/triage-flow' },
            { label: 'Unsubscribe', slug: 'guides/unsubscribe' },
            { label: 'Reading and Paper trail', slug: 'guides/reading-and-paper-trail' },
            { label: 'Mailbox', slug: 'guides/mailbox' },
          ],
        },
        {
          label: 'Clients and accounts',
          collapsed: true,
          items: [
            { label: 'Web app', slug: 'guides/web-app' },
            { label: 'Desktop app', slug: 'guides/desktop-app' },
            { label: 'Raycast', slug: 'guides/raycast' },
            { label: 'Accounts', slug: 'guides/accounts' },
            { label: 'Sound, key hints and touch', slug: 'guides/sound-hints-and-touch' },
            { label: 'Remote mailbox', slug: 'guides/remote-mailbox' },
            { label: 'Headless Linux', slug: 'guides/headless-linux' },
          ],
        },
        {
          label: 'Control and maintain',
          collapsed: true,
          items: [
            { label: 'Security and privacy', slug: 'guides/security-and-privacy' },
            { label: 'Automation contract', slug: 'guides/automation-contract' },
            { label: 'LLM features', slug: 'guides/llm-features' },
            { label: 'Activity log', slug: 'guides/activity-log' },
            { label: 'Backup and recovery', slug: 'guides/backup-recovery' },
            { label: 'Pairing authority', slug: 'guides/pairing-authority' },
            { label: 'Observability', slug: 'guides/observability' },
            { label: 'Troubleshooting', slug: 'troubleshooting' },
          ],
        },
        {
          label: 'Build on mxr',
          collapsed: true,
          items: [
            { label: 'Architecture', slug: 'guides/architecture' },
            { label: 'Adapter development', slug: 'guides/adapter-development' },
            { label: 'Public Rust crates', slug: 'guides/public-rust-crates' },
            { label: 'Trace model calls', slug: 'guides/trace-model-calls' },
            { label: 'Glossary', slug: 'guides/glossary' },
          ],
        },
        {
          label: 'Reference',
          collapsed: true,
          items: [
            {
              label: 'CLI',
              items: [
                { label: 'Overview', slug: 'reference/cli' },
                { label: 'Concepts', slug: 'reference/cli/concepts' },
                {
                  label: 'Commands',
                  collapsed: true,
                  items: [
                    {
                      autogenerate: {
                        directory: 'reference/cli',
                        collapsed: true,
                      },
                    },
                  ],
                },
              ],
            },
            { label: 'TUI', slug: 'reference/tui' },
            { label: 'Keybindings', slug: 'reference/keybindings' },
            { label: 'Desk and places', slug: 'reference/desk-and-places' },
            { label: 'Time phrases', slug: 'reference/time-phrases' },
            { label: 'Config', slug: 'reference/config' },
            { label: 'JSON output', slug: 'reference/json-output' },
            { label: 'HTTP bridge', slug: 'reference/bridge' },
            { label: 'MCP server', slug: 'reference/mcp' },
            { label: 'API explorer', link: '/reference/api-explorer/' },
            { label: 'Bug reports', slug: 'reference/bug-report' },
            { label: 'Adapters', slug: 'reference/adapters' },
            { label: 'Conformance tests', slug: 'reference/conformance' },
          ],
        },
      ],
    }),
  ],
});
