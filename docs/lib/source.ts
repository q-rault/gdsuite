import { docs, meta } from '@/.source';
import { loader } from 'fumadocs-core/source';
import { createMDXSource, resolveFiles } from 'fumadocs-mdx';

const src = createMDXSource(docs, meta);
Object.assign(src, { files: resolveFiles({ docs, meta }) });

export const source = loader({
  baseUrl: '/',
  source: src,
});
