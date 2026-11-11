import path from 'node:path';

export const DOCS_APP_ROOT = path.resolve(process.cwd());
export const AUTHORED_CONTENT_DIR = path.join(DOCS_APP_ROOT, 'content', 'docs');
export const GENERATED_ROOT = path.join(DOCS_APP_ROOT, '.generated');
export const GENERATED_CONTENT_DIR = path.join(GENERATED_ROOT, 'content', 'docs');
// Each product's docs/public is copied under public/products/<id> so imported
// pages can keep their absolute image paths; rewritten at import time.
export const IMPORTED_PUBLIC_DIR = path.join(DOCS_APP_ROOT, 'public', 'products');
