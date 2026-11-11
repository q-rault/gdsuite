import path from 'node:path';

export type ProductId = 'gdtest' | 'gdhealth';

export type ProductDocsSource = {
  id: ProductId;
  repoRoot: string;
  contentDir: string;
  metaFile: string;
  publicDir: string;
  routeBase: `/gdtest` | `/gdhealth`;
  title: string;
};

export type ImportedDocPage = {
  product: ProductId;
  sourcePath: string;
  outputPath: string;
  slug: string[];
  title: string;
  description?: string;
};

const workspaceRoot = path.resolve(process.cwd(), '..', '..');

export const productSources: Record<ProductId, ProductDocsSource> = {
  gdtest: {
    id: 'gdtest',
    repoRoot: path.join(workspaceRoot, 'gdtest'),
    contentDir: path.join(workspaceRoot, 'gdtest', 'docs', 'content', 'docs'),
    metaFile: path.join(workspaceRoot, 'gdtest', 'docs', 'content', 'docs', 'meta.json'),
    publicDir: path.join(workspaceRoot, 'gdtest', 'docs', 'public'),
    routeBase: '/gdtest',
    title: 'gdtest',
  },
  gdhealth: {
    id: 'gdhealth',
    repoRoot: path.join(workspaceRoot, 'gdhealth'),
    contentDir: path.join(workspaceRoot, 'gdhealth', 'docs', 'content', 'docs'),
    metaFile: path.join(workspaceRoot, 'gdhealth', 'docs', 'content', 'docs', 'meta.json'),
    publicDir: path.join(workspaceRoot, 'gdhealth', 'docs', 'public'),
    routeBase: '/gdhealth',
    title: 'gdhealth',
  },
};
