import { defineConfig } from 'astro/config';
import starlight from '@astrojs/starlight';

export default defineConfig({
  site: 'https://mxr.sh',
  integrations: [
    starlight({
      title: 'mxr',
      description: 'Local-first, CLI-first email for humans and agents. Read, search, draft, and send offline. Two-way Gmail/IMAP sync. Scriptable from your shell and operable by your agent.',
      disable404Route: true,
      social: [
        { icon: 'github', label: 'GitHub', href: 'https://github.com/planetaryescape/mxr' },
      ],
      customCss: [
        './src/styles/custom.css',
      ],
      head: [
        { tag: 'link', attrs: { rel: 'preconnect', href: 'https://fonts.googleapis.com' } },
        { tag: 'link', attrs: { rel: 'preconnect', href: 'https://fonts.gstatic.com', crossorigin: '' } },
        { tag: 'meta', attrs: { name: 'theme-color', content: '#0d0d0c' } },
        // Social cards / SEO. Starlight derives canonical, og:title/type/url/locale/
        // description/site_name, and twitter:card from `site` + page frontmatter; these
        // add the image and the bits it does not emit. Absolute URLs are required for og.
        { tag: 'meta', attrs: { property: 'og:image', content: 'https://mxr.sh/og.png' } },
        { tag: 'meta', attrs: { property: 'og:image:width', content: '1200' } },
        { tag: 'meta', attrs: { property: 'og:image:height', content: '630' } },
        { tag: 'meta', attrs: { property: 'og:image:type', content: 'image/png' } },
        { tag: 'meta', attrs: { property: 'og:image:alt', content: 'mxr — your inbox, on your computer. Local-first, CLI-first email.' } },
        { tag: 'meta', attrs: { name: 'twitter:image', content: 'https://mxr.sh/og.png' } },
        { tag: 'meta', attrs: { name: 'twitter:image:alt', content: 'mxr — your inbox, on your computer.' } },
        { tag: 'meta', attrs: { name: 'twitter:title', content: 'mxr — your inbox, on your computer' } },
        { tag: 'meta', attrs: { name: 'twitter:description', content: 'Local-first, CLI-first email. Read, search, draft, and send offline. Two-way Gmail/IMAP sync. Scriptable and agent-operable.' } },
        { tag: 'link', attrs: { rel: 'apple-touch-icon', sizes: '180x180', href: '/apple-touch-icon.png' } },
        { tag: 'meta', attrs: { name: 'author', content: 'planetaryescape' } },
        { tag: 'meta', attrs: { name: 'keywords', content: 'email client, local-first email, CLI email, terminal email client, TUI email, Gmail CLI, IMAP client, SMTP, Rust, MCP server, agent email, offline email, self-hosted email' } },
      ],
      sidebar: [
        {
          label: 'Start Here',
          items: [
            { label: 'Installation', slug: 'getting-started/install' },
            { label: 'Quick Start', slug: 'getting-started/quick-start' },
            { label: 'Examples', slug: 'examples' },
            { label: 'Gmail Setup', slug: 'getting-started/gmail-setup' },
            { label: 'IMAP / SMTP Setup', slug: 'getting-started/imap-smtp-setup' },
            { label: 'First Sync', slug: 'getting-started/first-sync' },
          ],
        },
        {
          label: 'Daily Use',
          items: [
            { label: 'Mailbox Workflow', slug: 'guides/mailbox' },
            { label: 'Triage Flow', slug: 'guides/triage-flow' },
            { label: 'Unsubscribe', slug: 'guides/unsubscribe' },
            { label: 'Compose', slug: 'guides/compose' },
            { label: 'Pre-send Safety', slug: 'guides/pre-send-safety' },
            { label: 'Calendar Invites', slug: 'guides/calendar-invites' },
            { label: 'Search Workflow', slug: 'guides/search' },
            { label: 'Labels and Saved Searches', slug: 'guides/labels-and-saved-searches' },
            { label: 'Sender View', slug: 'guides/sender-view' },
            { label: 'Snippets', slug: 'guides/snippets' },
            { label: 'Web App', slug: 'guides/web-app' },
            { label: 'No Native Desktop App', slug: 'guides/no-native-desktop-app' },
            { label: 'Recipes (fzf, jq, xargs, cron)', slug: 'guides/recipes' },
          ],
        },
        {
          label: 'Power Features',
          items: [
            { label: 'Automated Follow-ups', slug: 'guides/automated-followups' },
            { label: 'Forgotten Work', slug: 'guides/forgotten-work' },
            { label: 'Archive Intelligence', slug: 'guides/archive-intelligence' },
            { label: 'Deliveries', slug: 'guides/deliveries' },
            { label: 'Timing and Cadence', slug: 'guides/timing-and-cadence' },
            { label: 'Briefings and Loop-in', slug: 'guides/briefings-and-loop-in' },
            { label: 'Rules', slug: 'guides/rules' },
            { label: 'LLM Features', slug: 'guides/llm-features' },
            { label: 'Semantic Search', slug: 'guides/semantic-search' },
            { label: 'Analytics', slug: 'guides/analytics' },
            { label: 'Crash-Safe Drafts', slug: 'guides/crash-safe-drafts' },
            { label: 'Activity Log', slug: 'guides/activity-log' },
            { label: 'Accounts', slug: 'guides/accounts' },
          ],
        },
        {
          label: 'Concepts',
          items: [
            { label: 'Why mxr', slug: 'guides/why-mxr' },
            { label: 'Architecture', slug: 'guides/architecture' },
            { label: 'Security & Privacy', slug: 'guides/security-and-privacy' },
            { label: 'Glossary', slug: 'guides/glossary' },
            { label: 'For Agents', slug: 'guides/for-agents' },
            { label: 'Automation Contract', slug: 'guides/automation-contract' },
            { label: 'Observability', slug: 'guides/observability' },
          ],
        },
        {
          label: 'Building on mxr',
          items: [
            { label: 'Adapter Development', slug: 'guides/adapter-development' },
            { label: 'AI Agent Skill', slug: 'guides/agent-skill' },
            { label: 'Public Rust crates', slug: 'guides/public-rust-crates' },
          ],
        },
        {
          label: 'Reference',
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
            { label: 'Config', slug: 'reference/config' },
            { label: 'JSON output schemas', slug: 'reference/json-output' },
            { label: 'HTTP Bridge', slug: 'reference/bridge' },
            { label: 'MCP Server', slug: 'reference/mcp' },
            { label: 'API Explorer', link: '/reference/api-explorer/' },
            { label: 'Bug Reports', slug: 'reference/bug-report' },
            { label: 'Adapters', slug: 'reference/adapters' },
            { label: 'Conformance Tests', slug: 'reference/conformance' },
          ],
        },
        {
          label: 'Help',
          items: [
            { label: 'Troubleshooting', slug: 'troubleshooting' },
          ],
        },
      ],
    }),
  ],
});
