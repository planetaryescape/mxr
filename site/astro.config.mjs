import { defineConfig } from 'astro/config';
import starlight from '@astrojs/starlight';

export default defineConfig({
  integrations: [
    starlight({
      title: 'mxr',
      description: 'Local-first email infrastructure for humans and agents',
      social: {
        github: 'https://github.com/planetaryescape/mxr',
      },
      customCss: [
        './src/styles/custom.css',
      ],
      sidebar: [
        {
          label: 'Start Here',
          items: [
            { label: 'Installation', slug: 'getting-started/install' },
            { label: 'Gmail Setup', slug: 'getting-started/gmail-setup' },
            { label: 'IMAP / SMTP Setup', slug: 'getting-started/imap-smtp-setup' },
            { label: 'First Sync', slug: 'getting-started/first-sync' },
          ],
        },
        {
          label: 'Guides',
          items: [
            { label: 'Why mxr', slug: 'guides/why-mxr' },
            { label: 'Architecture', slug: 'guides/architecture' },
            { label: 'For Agents', slug: 'guides/for-agents' },
            { label: 'Security & Privacy', slug: 'guides/security-and-privacy' },
            { label: 'Mailbox Workflow', slug: 'guides/mailbox' },
            { label: 'Compose', slug: 'guides/compose' },
            { label: 'Search Workflow', slug: 'guides/search' },
            { label: 'Labels and Saved Searches', slug: 'guides/labels-and-saved-searches' },
            { label: 'Rules', slug: 'guides/rules' },
            { label: 'Accounts', slug: 'guides/accounts' },
            { label: 'Observability', slug: 'guides/observability' },
            { label: 'Adapter Development', slug: 'guides/adapter-development' },
            { label: 'AI Agent Skill', slug: 'guides/agent-skill' },
          ],
        },
        {
          label: 'Reference',
          items: [
            { label: 'CLI Commands', slug: 'reference/cli' },
            { label: 'TUI', slug: 'reference/tui' },
            { label: 'Keybindings', slug: 'reference/keybindings' },
            { label: 'Config', slug: 'reference/config' },
            { label: 'Bug Reports', slug: 'reference/bug-report' },
            { label: 'Adapters', slug: 'reference/adapters' },
            { label: 'Conformance Tests', slug: 'reference/conformance' },
          ],
        },
      ],
    }),
  ],
});
