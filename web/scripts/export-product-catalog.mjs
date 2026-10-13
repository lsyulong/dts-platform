import { readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import ts from 'typescript';

const web = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const loaded = new Map();
async function loadCatalog(name) {
  const file = path.join(web, 'src/product', `${name}.ts`);
  if (loaded.has(file)) return loaded.get(file);
  const source = await readFile(file, 'utf8');
  const compiled = ts.transpileModule(source, {
    compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS },
  }).outputText;
  const dependencies = new Map();
  for (const match of compiled.matchAll(/require\("\.\/([^"/]+)"\)/g)) {
    dependencies.set(`./${match[1]}`, await loadCatalog(match[1]));
  }
  const exports = {};
  new Function('exports', 'require', compiled)(exports, (id) => {
    if (!dependencies.has(id)) throw new Error(`Unexpected catalog dependency: ${id}`);
    return dependencies.get(id);
  });
  loaded.set(file, exports);
  return exports;
}
const { productGroups: groups, productModules: modules } = await loadCatalog('catalog');
for (const key of ['id', 'path']) {
  if (new Set(modules.map((module) => module[key])).size !== modules.length)
    throw new Error(`Duplicate module ${key}`);
}
const destination = path.resolve(web, '../crates/dts-server/resources/product/catalog.json');
const content = `${JSON.stringify({ groups, modules }, null, 2)}\n`;
if (process.argv.includes('--check')) {
  if ((await readFile(destination, 'utf8')) !== content)
    throw new Error(
      'Backend product catalog is stale; run node scripts/export-product-catalog.mjs',
    );
} else {
  await writeFile(destination, content);
}
console.log(`Product contract: ${groups.length} groups, ${modules.length} modules`);
