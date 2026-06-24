import type { ReactNode } from 'react';
import { baseOptions } from '@/app/layout.config';
import { source } from '@/lib/source';
import { DocsLayoutClient } from './docs-layout-client';

export default function Layout({ children }: { children: ReactNode }) {
  return (
    <DocsLayoutClient tree={source.pageTree} {...baseOptions}>
      {children}
    </DocsLayoutClient>
  );
}
