import path from 'node:path';

export const DOCS_APP_ROOT = path.resolve(process.cwd());
export const AUTHORED_CONTENT_DIR = path.join(DOCS_APP_ROOT, 'content', 'docs');
export const GENERATED_ROOT = path.join(DOCS_APP_ROOT, '.generated');
export const GENERATED_CONTENT_DIR = path.join(GENERATED_ROOT, 'content', 'docs');
