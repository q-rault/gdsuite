import { defineDocs, defineConfig } from 'fumadocs-mdx/config';

export const { docs, meta } = defineDocs({
  dir: '.generated/content/docs',
});

export default defineConfig({});
