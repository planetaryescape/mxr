import { defineConfig } from 'astro/config';
import starlight from '@astrojs/starlight';

export default defineConfig({
  site: 'https://mxr-mail.vercel.app',
  integrations: [
    starlight({
      title: 'mxr',
      description: 'Local-first email infrastructure for humans and agents',
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
      ],
      sidebar: [
        {
          label: 'Start Here',
          items: [
            { label: 'Installation', slug: 'getting-started/install' },
            { label: 'Quick Start', slug: 'getting-started/quick-start' },
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
            { label: 'Compose', slug: 'guides/compose' },
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
            { label: 'Rules', slug: 'guides/rules' },
            { label: 'LLM Features', slug: 'guides/llm-features' },
            { label: 'Semantic Search', slug: 'guides/semantic-search' },
            { label: 'Analytics', slug: 'guides/analytics' },
            { label: 'Crash-Safe Drafts', slug: 'guides/crash-safe-drafts' },
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
                  autogenerate: { directory: 'reference/cli' },
                },
              ],
            },
            { label: 'TUI', slug: 'reference/tui' },
            { label: 'Keybindings', slug: 'reference/keybindings' },
            { label: 'Config', slug: 'reference/config' },
            { label: 'JSON output schemas', slug: 'reference/json-output' },
            { label: 'HTTP Bridge', slug: 'reference/bridge' },
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
