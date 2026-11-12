import path from 'node:path';

export type ProductId = 'gdtest' | 'gdhealth' | 'gdtidy' | 'gdtypecheck';

export type ProductDocsSource = {
  id: ProductId;
  repoRoot: string;
  contentDir: string;
  publicDir: string;
  /** The tool's own docs site, where pages the portal doesn't import live. */
  siteUrl: string;
  /** How a command of this tool is written in a GDSuite project. */
  suiteCommand: { tool: string; suite: string }[];
};

/**
 * A tool page shown in the portal. The portal imports only the pages a
 * GDSuite user needs (writing tests, rules, settings); a tool's own install,
 * quickstart and overview pages stay on its site.
 */
export type ImportSpec = {
  product: ProductId;
  /** Page id in the tool's docs/content/docs. */
  page: string;
  /** Route in the portal, without leading slash. */
  to: string;
  /** Replaces the page's title, to fit the portal's navigation. */
  title?: string;
};

export type ImportedDocPage = {
  product: ProductId;
  sourcePath: string;
  outputPath: string;
  route: string;
  title: string;
  description?: string;
};

const workspaceRoot = path.resolve(process.cwd(), '..', '..');

function source(
  id: ProductId,
  suiteCommand: ProductDocsSource['suiteCommand'],
): ProductDocsSource {
  return {
    id,
    repoRoot: path.join(workspaceRoot, id),
    contentDir: path.join(workspaceRoot, id, 'docs', 'content', 'docs'),
    publicDir: path.join(workspaceRoot, id, 'docs', 'public'),
    siteUrl: `https://q-rault.github.io/${id}/docs`,
    suiteCommand,
  };
}

export const productSources: Record<ProductId, ProductDocsSource> = {
  gdtest: source('gdtest', [{ tool: 'gdtest', suite: 'gdsuite test --' }]),
  gdhealth: source('gdhealth', [{ tool: 'gdhealth', suite: 'gdsuite health --' }]),
  gdtidy: source('gdtidy', [
    { tool: 'gdtidy fmt', suite: 'gdsuite fmt --' },
    { tool: 'gdtidy lint', suite: 'gdsuite lint --' },
  ]),
  gdtypecheck: source('gdtypecheck', [{ tool: 'gdtypecheck', suite: 'gdsuite typecheck --' }]),
};

export const importSpecs: ImportSpec[] = [
  // Writing tests
  { product: 'gdtest', page: 'guide-first-test', to: 'writing-tests/first-test', title: 'Your first test' },
  { product: 'gdtest', page: 'scene-runner', to: 'writing-tests/scenes', title: 'Testing scenes and input' },
  { product: 'gdtest', page: 'mocking', to: 'writing-tests/mocks', title: 'Replacing dependencies' },
  { product: 'gdtest', page: 'snapshots', to: 'writing-tests/snapshots', title: 'Snapshot tests' },
  { product: 'gdtest', page: 'running-tests', to: 'writing-tests/running', title: 'Filters, tags and coverage' },

  // Adding GDSuite to an existing project
  { product: 'gdtidy', page: 'migrate-from-gdtoolkit', to: 'existing-project/from-gdtoolkit', title: 'Coming from gdtoolkit' },
  { product: 'gdtest', page: 'migrate-from-gut', to: 'existing-project/from-gut', title: 'Coming from GUT' },
  { product: 'gdtest', page: 'migrate-from-gdunit4', to: 'existing-project/from-gdunit4', title: 'Coming from gdUnit4' },

  // Reference
  { product: 'gdtest', page: 'assertions', to: 'reference/assertions', title: 'Assertions' },
  { product: 'gdtest', page: 'ref-mock-api', to: 'reference/mock-api', title: 'Mock API' },
  { product: 'gdtest', page: 'ref-scene-runner-api', to: 'reference/scene-runner-api', title: 'Scene runner API' },
  { product: 'gdtidy', page: 'rules', to: 'reference/lint-rules', title: 'Lint rules' },
  { product: 'gdtidy', page: 'formatting', to: 'reference/formatting-style', title: 'Formatting style' },
  { product: 'gdtypecheck', page: 'what-it-reports', to: 'reference/type-check-findings', title: 'Type check findings' },
  { product: 'gdhealth', page: 'interpreting-reports', to: 'reference/health-metrics', title: 'Health metrics' },
  { product: 'gdtest', page: 'configuration', to: 'reference/test-settings', title: 'Test settings' },
  { product: 'gdtidy', page: 'configuration', to: 'reference/format-lint-settings', title: 'Format and lint settings' },
  { product: 'gdhealth', page: 'configuration', to: 'reference/health-settings', title: 'Health settings' },
];
