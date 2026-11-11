import fs from 'node:fs';
import path from 'node:path';
import {
  AUTHORED_CONTENT_DIR,
  GENERATED_CONTENT_DIR,
  IMPORTED_PUBLIC_DIR,
} from './imported-docs';
import {
  importSpecs,
  productSources,
  type ImportSpec,
  type ImportedDocPage,
  type ProductDocsSource,
  type ProductId,
} from './product-sources';

const FRONTMATTER_RE = /^---\r?\n([\s\S]*?)\r?\n---\r?\n?/;
const CODE_FENCE_WITH_FILE_RE =
  /```(\w+)([^\n`]*?)\sfile="([^"]+)"(?:\sregion="([^"]+)")?([^\n`]*)\n([\s\S]*?)```/g;
const SHELL_FENCE_RE = /```(?:sh|bash|shell|console)[^\n]*\n([\s\S]*?)```/g;
// `/docs/page`, `/docs/page#a`, and the tool's home: `/docs`, `/docs#a`.
const ABSOLUTE_DOC_LINK_RE = /\]\(\/docs(?:\/([^)#]*))?(#[^)]+)?\)/g;
const RELATIVE_DOC_LINK_RE = /\]\(((?:\.\.\/|\.\/)?[^)#]+?)(#[^)]+)?\)/g;
const ABSOLUTE_IMAGE_RE = /(!\[[^\]]*\]\()(\/[^)\s]+)\)/g;
const RELATIVE_IMPORT_RE =
  /^\s*import\s.+?from\s+['"](\.\/|\.\.\/)/m;

function main() {
  prepareGeneratedTree();

  const available = new Set<ProductId>();
  for (const source of Object.values(productSources)) {
    // A tool release from before its docs site has no pages to import.
    if (!fs.existsSync(source.contentDir)) {
      console.warn(`${source.id} has no docs at ${source.contentDir}; its pages are left out`);
      continue;
    }
    available.add(source.id);
    if (fs.existsSync(source.publicDir)) {
      copyDir(source.publicDir, path.join(IMPORTED_PUBLIC_DIR, source.id));
    }
  }

  const imported: ImportedDocPage[] = [];
  for (const spec of importSpecs) {
    if (!available.has(spec.product)) continue;
    imported.push(importPage(spec));
  }

  console.log(`synced docs content: ${imported.length} imported tool pages`);
}

function prepareGeneratedTree() {
  fs.rmSync(GENERATED_CONTENT_DIR, { recursive: true, force: true });
  fs.rmSync(IMPORTED_PUBLIC_DIR, { recursive: true, force: true });
  copyDir(AUTHORED_CONTENT_DIR, GENERATED_CONTENT_DIR);
}

function importPage(spec: ImportSpec): ImportedDocPage {
  const source = productSources[spec.product];
  const sourcePath = path.join(source.contentDir, `${spec.page}.mdx`);
  assertPathExists(sourcePath, `${source.id} has no page ${spec.page}.mdx at ${sourcePath}`);

  const original = fs.readFileSync(sourcePath, 'utf8');
  if (RELATIVE_IMPORT_RE.test(original)) {
    throw new Error(
      `${source.id} page ${sourcePath} uses a relative import; imported pages must be self-contained MDX.`,
    );
  }

  let rewritten = rewriteImportedPage(source, original, routeFor(source));
  if (spec.title) rewritten = replaceTitle(rewritten, spec.title);
  rewritten = addSuiteNote(source, rewritten);

  const outputPath = path.join(GENERATED_CONTENT_DIR, `${spec.to}.mdx`);
  if (fs.existsSync(outputPath)) {
    throw new Error(`${spec.to} is both an authored page and imported from ${source.id}`);
  }
  fs.mkdirSync(path.dirname(outputPath), { recursive: true });
  fs.writeFileSync(outputPath, rewritten);

  const { title, description } = parseFrontmatter(rewritten);
  return { product: source.id, sourcePath, outputPath, route: `/${spec.to}`, title, description };
}

/** Where a page of a tool lives: in the portal when imported, else on the tool's site. */
function routeFor(source: ProductDocsSource): (pageId: string) => string {
  const imported = new Map(
    importSpecs
      .filter((spec) => spec.product === source.id)
      .map((spec) => [spec.page, `/${spec.to}`] as const),
  );
  return (pageId) => {
    if (pageId === '' || pageId === 'index') return source.siteUrl;
    return imported.get(pageId) ?? `${source.siteUrl}/${pageId}`;
  };
}

function replaceTitle(contents: string, title: string): string {
  return contents.replace(FRONTMATTER_RE, (block) =>
    block.replace(/^title:.*$/m, `title: ${JSON.stringify(title)}`),
  );
}

/**
 * A page that shows the tool's own commands says how to run them through
 * gdsuite, so a GDSuite user never thinks they have to install the tool.
 */
function addSuiteNote(source: ProductDocsSource, contents: string): string {
  const shell = [...contents.matchAll(SHELL_FENCE_RE)].map((match) => match[1]).join('\n');
  const used = source.suiteCommand.filter(({ tool }) =>
    new RegExp(`^\\s*${tool.replace(/ /g, '\\s+')}\\b`, 'm').test(shell),
  );
  if (used.length === 0) return contents;

  const examples = used
    .map(({ tool, suite }) => `\`${tool} <arguments>\` becomes \`${suite} <arguments>\``)
    .join(', and ');
  const note = [
    '<Callout title="In a GDSuite project">',
    '',
    `The commands on this page call ${source.id} directly. GDSuite installs ${source.id} for you and passes it the same arguments: ${examples}.`,
    '',
    '</Callout>',
    '',
  ].join('\n');

  const frontmatter = contents.match(FRONTMATTER_RE);
  if (!frontmatter) return `${note}\n${contents}`;
  const end = frontmatter[0].length;
  return `${contents.slice(0, end)}\n${note}\n${contents.slice(end).replace(/^\n+/, '')}`;
}

function rewriteImportedPage(
  source: ProductDocsSource,
  contents: string,
  route: (pageId: string) => string,
): string {
  const withInlineSnippets = contents.replace(
    CODE_FENCE_WITH_FILE_RE,
    (_match, lang, beforeFile, fileAttr, region, afterRegion) => {
      const resolved = resolveSnippetPath(source, fileAttr);
      const snippetSource = fs.readFileSync(resolved, 'utf8');
      const snippet = region ? extractRegion(snippetSource, region, resolved) : snippetSource.trimEnd();
      const meta = `${beforeFile}${afterRegion}`.trim();
      const metaSuffix = meta.length > 0 ? ` ${meta}` : '';
      return `\`\`\`${lang}${metaSuffix}\n${snippet}\n\`\`\``;
    },
  );

  const rewrittenAbsolute = withInlineSnippets.replace(
    ABSOLUTE_DOC_LINK_RE,
    (_match, target = '', hash = '') => {
      const pageId = target.replace(/\/+$/, '').split('/').pop() ?? '';
      return `](${route(pageId)}${hash})`;
    },
  );

  // An image at /x.png in the tool's site lives at /products/<id>/x.png here.
  const rewrittenImages = rewrittenAbsolute.replace(
    ABSOLUTE_IMAGE_RE,
    (match, prefix, target) => {
      if (!fs.existsSync(path.join(source.publicDir, target))) return match;
      return `${prefix}/products/${source.id}${target})`;
    },
  );

  return rewrittenImages.replace(RELATIVE_DOC_LINK_RE, (match, target, hash = '') => {
    if (
      target.startsWith('http://') ||
      target.startsWith('https://') ||
      target.startsWith('mailto:') ||
      target.startsWith('#') ||
      target.startsWith('/')
    ) {
      return match;
    }

    const normalized = target.replace(/^\.\//, '').replace(/\/index$/, '').replace(/\.mdx?$/, '');
    const pageId = path.basename(normalized);
    if (!fs.existsSync(path.join(source.contentDir, `${pageId}.mdx`))) return match;
    return `](${route(pageId)}${hash})`;
  });
}

function resolveSnippetPath(source: ProductDocsSource, rawPath: string): string {
  const candidates = [
    path.resolve(source.repoRoot, rawPath),
    path.resolve(source.contentDir, '..', '..', rawPath),
  ];

  for (const candidate of candidates) {
    if (fs.existsSync(candidate)) return candidate;
  }

  throw new Error(`unable to resolve snippet file "${rawPath}" for ${source.id}`);
}

function extractRegion(source: string, region: string, filePath: string): string {
  const lines = source.split('\n');
  const start = lines.findIndex((line) => line.trim() === `# region: ${region}`);
  if (start === -1) throw new Error(`region "${region}" not found in ${filePath}`);
  const end = lines.findIndex((line, index) => index > start && line.trim() === '# endregion');
  if (end === -1) throw new Error(`missing # endregion after region "${region}" in ${filePath}`);
  return lines.slice(start + 1, end).join('\n').trimEnd();
}

function parseFrontmatter(contents: string): { title: string; description?: string } {
  const match = contents.match(FRONTMATTER_RE);
  if (!match) {
    return { title: 'Untitled' };
  }

  const lines = match[1].split(/\r?\n/);
  const values: Record<string, string> = {};
  for (const line of lines) {
    const separator = line.indexOf(':');
    if (separator === -1) continue;
    const key = line.slice(0, separator).trim();
    const value = line.slice(separator + 1).trim().replace(/^['"]|['"]$/g, '');
    values[key] = value;
  }

  return {
    title: values.title ?? 'Untitled',
    description: values.description,
  };
}

function copyDir(sourceDir: string, targetDir: string) {
  fs.mkdirSync(targetDir, { recursive: true });
  for (const entry of fs.readdirSync(sourceDir, { withFileTypes: true })) {
    const sourcePath = path.join(sourceDir, entry.name);
    const targetPath = path.join(targetDir, entry.name);
    if (entry.isDirectory()) {
      copyDir(sourcePath, targetPath);
    } else {
      fs.copyFileSync(sourcePath, targetPath);
    }
  }
}

function assertPathExists(targetPath: string, message: string) {
  if (!fs.existsSync(targetPath)) {
    throw new Error(message);
  }
}

main();
