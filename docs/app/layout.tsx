import { RootProvider } from 'fumadocs-ui/provider';
import { Space_Grotesk } from 'next/font/google';
import type { ReactNode } from 'react';
import './global.css';

const spaceGrotesk = Space_Grotesk({ subsets: ['latin'] });

export const metadata = {
  title: {
    template: '%s | GDSuite Docs',
    default: 'GDSuite Docs | Suite-first Godot workflow',
  },
  description:
    'Guide-first docs for gdsuite, gdtest, and gdhealth in one portal.',
};

export default function RootLayout({ children }: { children: ReactNode }) {
  return (
    <html lang="en" className={spaceGrotesk.className} suppressHydrationWarning>
      <body className="min-h-screen bg-[radial-gradient(circle_at_top,rgba(210,230,255,0.4),transparent_40%),linear-gradient(180deg,#fffdf8_0%,#ffffff_18%,#f8fbff_100%)] text-slate-950">
        <RootProvider>{children}</RootProvider>
      </body>
    </html>
  );
}
