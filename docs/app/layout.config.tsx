import type { BaseLayoutProps } from 'fumadocs-ui/layouts/shared';
import { GitHubIcon } from '@/components/github-icon';

export const baseOptions: BaseLayoutProps = {
  nav: {
    title: 'GDSuite Docs',
  },
  links: [
    {
      text: 'Guide',
      url: '/',
      active: 'nested-url',
    },
    {
      text: 'Config',
      url: '/config',
      active: 'nested-url',
    },
    {
      text: 'Products',
      url: '/gdtest',
      active: 'nested-url',
    },
    {
      text: 'Recipes',
      url: '/recipes',
      active: 'nested-url',
    },
    {
      text: 'Reference',
      url: '/reference/cli',
      active: 'nested-url',
    },
    {
      text: 'GitHub',
      url: 'https://github.com/q-rault/gdsuite',
      external: true,
      secondary: true,
      icon: <GitHubIcon />,
    },
  ],
};
