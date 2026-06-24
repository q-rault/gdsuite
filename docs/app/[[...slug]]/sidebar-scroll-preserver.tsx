'use client';

import { useEffect } from 'react';
import { usePathname } from 'next/navigation';

function getRootSegment(pathname: string): string {
  return pathname.split('/').filter(Boolean)[0] ?? '__root__';
}

const savedScrolls: Record<string, number> = {};

export function SidebarScrollPreserver() {
  const pathname = usePathname();

  useEffect(() => {
    const root = getRootSegment(pathname);

    const viewport = document.querySelector<HTMLElement>(
      '[data-radix-scroll-area-viewport]',
    );
    if (!viewport) return;

    viewport.scrollTop = savedScrolls[root] ?? 0;

    const handleScroll = () => {
      savedScrolls[root] = viewport.scrollTop;
    };

    viewport.addEventListener('scroll', handleScroll, { passive: true });
    return () => viewport.removeEventListener('scroll', handleScroll);
  }, [pathname]);

  return null;
}
