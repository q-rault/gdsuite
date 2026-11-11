'use client';

import { DocsLayout } from 'fumadocs-ui/layouts/docs';
import type { BaseLayoutProps } from 'fumadocs-ui/layouts/shared';
import type * as PageTree from 'fumadocs-core/page-tree';
import type { ReactNode } from 'react';
import { SidebarScrollPreserver } from './sidebar-scroll-preserver';
import { SidebarItem } from 'fumadocs-ui/components/layout/sidebar';
import { VersionSwitcher } from '@/components/version-switcher';

type StaticFolderProps = {
  item: PageTree.Folder;
  level: number;
  children: ReactNode;
};

function StaticFolder({ item, children }: StaticFolderProps) {
  return (
    <>
      {item.index && (
        <SidebarItem
          href={item.index.url}
          icon={item.index.icon ?? item.icon}
          className="py-1.5"
        >
          {item.index.name}
        </SidebarItem>
      )}
      {children}
    </>
  );
}

function CompactItem({ item }: { item: PageTree.Item }) {
  return (
    <SidebarItem
      href={item.url}
      external={item.external}
      icon={item.icon}
      className="py-1.5"
    >
      {item.name}
    </SidebarItem>
  );
}

const SIDEBAR_TABS = [
  { url: '/', title: 'GDSuite Docs' },
  { url: '/gdtest', title: 'gdtest' },
  { url: '/gdhealth', title: 'gdhealth' },
  { url: '/gdtidy', title: 'gdtidy' },
  { url: '/gdtypecheck', title: 'gdtypecheck' },
];

type Props = BaseLayoutProps & {
  tree: PageTree.Root;
  children: ReactNode;
};

export function DocsLayoutClient({ tree, children, ...props }: Props) {
  return (
    <>
      <SidebarScrollPreserver />
      <DocsLayout
        tree={tree}
        {...props}
        sidebar={{
          tabs: SIDEBAR_TABS,
          banner: <VersionSwitcher />,
          defaultOpenLevel: Number.POSITIVE_INFINITY,
          components: {
            Folder: StaticFolder,
            Item: CompactItem,
          },
        }}
      >
        {children}
      </DocsLayout>
    </>
  );
}
