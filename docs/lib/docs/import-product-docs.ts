import fs from 'node:fs';
import path from 'node:path';
import {
  AUTHORED_CONTENT_DIR,
  GENERATED_CONTENT_DIR,
} from './imported-docs';
import {
  productSources,
  type ImportedDocPage,
  type ProductDocsSource,
} from './product-sources';

type MetaFile = {
  title: string;
  pages: string[];
};

const FRONTMATTER_RE = /^---\r?\n([\s\S]*?)\r?\n---\r?\n?/;
const CODE_FENCE_WITH_FILE_RE =
  /```(\w+)([^\n`]*?)\sfile="([^"]+)"(?:\sregion="([^"]+)")?([^\n`]*)\n([\s\S]*?)```/g;
const ABSOLUTE_DOC_LINK_RE = /\]\(\/docs\/([^)#]+)(#[^)]+)?\)/g;
const RELATIVE_DOC_LINK_RE = /\]\(((?:\.\.\/|\.\/)?[^)#]+?)(#[^)]+)?\)/g;
const RELATIVE_IMPORT_RE =
  /^\s*import\s.+?from\s+['"](\.\/|\.\.\/)/m;

function main() {
  prepareGeneratedTree();

  const importedPages: ImportedDocPage[] = [];
  for (const source of Object.values(productSources)) {
    importedPages.push(...importProductDocs(source));
  }

  console.log(`synced docs content: ${importedPages.length} imported product pages`);
}

function prepareGeneratedTree() {
  fs.rmSync(GENERATED_CONTENT_DIR, { recursive: true, force: true });
  copyDir(AUTHORED_CONTENT_DIR, GENERATED_CONTENT_DIR);
}

function importProductDocs(source: ProductDocsSource): ImportedDocPage[] {
  assertPathExists(source.repoRoot, `missing sibling repo for ${source.id} at ${source.repoRoot}`);
  assertPathExists(
    source.contentDir,
    `missing docs content directory for ${source.id} at ${source.contentDir}`,
  );
  assertPathExists(source.metaFile, `missing meta.json for ${source.id} at ${source.metaFile}`);

  const meta = JSON.parse(fs.readFileSync(source.metaFile, 'utf8')) as MetaFile;
  const outputDir = path.join(GENERATED_CONTENT_DIR, source.id);
  fs.mkdirSync(outputDir, { recursive: true });

  const routeMap = buildRouteMap(source, meta);
  const imported: ImportedDocPage[] = [];

  for (const pageId of meta.pages) {
    if (pageId.startsWith('---') || pageId === 'index') continue;
    const sourcePath = path.join(source.contentDir, `${pageId}.mdx`);
    assertPathExists(
      sourcePath,
      `${source.id} meta.json references missing page ${pageId}.mdx at ${sourcePath}`,
    );

    const original = fs.readFileSync(sourcePath, 'utf8');
    if (RELATIVE_IMPORT_RE.test(original)) {
      throw new Error(
        `${source.id} page ${sourcePath} uses a relative import. Imported docs only support self-contained MDX pages in phase 1.`,
      );
    }

    const rewritten = rewriteImportedPage(source, original, routeMap);
    const { title, description } = parseFrontmatter(rewritten);
    const outputPath = path.join(outputDir, `${pageId}.mdx`);
    fs.writeFileSync(outputPath, rewritten);

    imported.push({
      product: source.id,
      sourcePath,
      outputPath,
      slug: [source.id, pageId],
      title,
      description,
    });
  }

  const productMetaPath = path.join(outputDir, 'meta.json');
  const generatedMeta = {
    title: source.title,
    pages: ['index', ...meta.pages.filter((entry) => entry !== 'index')],
  };
  fs.writeFileSync(productMetaPath, `${JSON.stringify(generatedMeta, null, 2)}\n`);

  return imported;
}

function buildRouteMap(source: ProductDocsSource, meta: MetaFile): Map<string, string> {
  const map = new Map<string, string>();
  for (const pageId of meta.pages) {
    if (pageId.startsWith('---')) continue;
    if (pageId === 'index') {
      map.set(pageId, source.routeBase);
      continue;
    }
    map.set(pageId, `${source.routeBase}/${pageId}`);
  }
  return map;
}

function rewriteImportedPage(
  source: ProductDocsSource,
  contents: string,
  routeMap: Map<string, string>,
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
    (_match, target, hash = '') => {
      const normalized = target.replace(/\/+$/, '');
      const slug = normalized.split('/').pop();
      const mapped = routeMap.get(slug ?? '');
      return `](${mapped ?? `${source.routeBase}/${normalized}`}${hash})`;
    },
  );

  return rewrittenAbsolute.replace(RELATIVE_DOC_LINK_RE, (match, target, hash = '') => {
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
    const mapped = routeMap.get(pageId);
    return mapped ? `](${mapped}${hash})` : match;
  });
}

function resolveSnippetPath(source: ProductDocsSource, rawPath: string): string {
  const candidates = [
    path.resolve(source.repoRoot, rawPath),
    path.resolve(path.dirname(source.metaFile), '..', '..', rawPath),
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
