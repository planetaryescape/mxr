import { defineConfig } from 'astro/config';
import starlight from '@astrojs/starlight';

export default defineConfig({
  integrations: [
    starlight({
      title: 'mxr',
      description: 'A local-first, keyboard-native terminal email client',
      social: [
        { icon: 'github', label: 'GitHub', href: 'https://github.com/planetaryescape/mxr' }
      ],
      sidebar: [
        {
          label: 'Getting Started',
          items: [
            { label: 'Installation', slug: 'getting-started/install' },
            { label: 'Gmail Setup', slug: 'getting-started/gmail-setup' },
          ],
        },
        {
          label: 'Reference',
          items: [
            { label: 'CLI Commands', slug: 'reference/cli' },
            { label: 'Keybindings', slug: 'reference/keybindings' },
          ],
        },
      ],
    }),
  ],
});
