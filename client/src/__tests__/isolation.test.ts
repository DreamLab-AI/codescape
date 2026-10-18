/**
 * Codescape must stay a separate client (BASELINE I9): walk the import graph
 * of every source file under client/ and fail on any import that leaves this
 * directory. There are no exceptions; the wire fixture the protocol tests use
 * is a committed byte file produced by visionclaw-wire. Bare imports are
 * limited to the allowlisted packages in client/package.json.
 */
import { describe, expect, it } from 'vitest';

// Loaded at runtime rather than imported: under a jsdom environment Vite would
// replace node builtins with browser stubs, and the suite shares the jsdom
// environment with every other test.
type Fs = typeof import('node:fs');
type Path = typeof import('node:path');
const fs = process.getBuiltinModule('node:fs') as Fs;
const path = process.getBuiltinModule('node:path') as Path;

const ROOT = path.resolve(__dirname, '..', '..'); // client/
const ALLOWED_PACKAGES = [/^react$/, /^react\/jsx-runtime$/, /^react-dom\/client$/, /^three$/, /^three\/examples\/jsm\//, /^@react-three\/fiber$/, /^@react-three\/drei$/, /^vitest(\/config)?$/, /^vite$/, /^@vitejs\/plugin-react$/, /^node:/];
const EXTS = ['', '.ts', '.tsx', '.js', '.mjs', '/index.ts', '/index.tsx'];

function listSources(dir: string): string[] {
  const out: string[] = [];
  for (const e of fs.readdirSync(dir, { withFileTypes: true })) {
    if (e.name === 'node_modules' || e.name.startsWith('.')) continue;
    const p = path.join(dir, e.name);
    if (e.isDirectory()) out.push(...listSources(p));
    else if (/\.(ts|tsx)$/.test(e.name)) out.push(p);
  }
  return out;
}

/** Static specifiers of import/export-from statements, dynamic imports and worker URL constructors. */
export function specifiers(src: string): string[] {
  const out: string[] = [];
  const res = [
    /\b(?:import|export)\s+(?:type\s+)?(?:[^'"`;]*?\s+from\s+)?['"]([^'"]+)['"]/g,
    /\bimport\(\s*['"]([^'"]+)['"]\s*\)/g,
    /new\s+URL\(\s*['"]([^'"]+)['"]\s*,\s*import\.meta\.url\s*\)/g,
  ];
  for (const re of res) for (const m of src.matchAll(re)) out.push(m[1]);
  return out;
}

function resolveRelative(from: string, spec: string): string | null {
  const base = path.resolve(path.dirname(from), spec);
  for (const ext of EXTS) {
    const p = base + ext;
    if (fs.existsSync(p) && fs.statSync(p).isFile()) return p;
  }
  return null;
}

describe('import isolation', () => {
  it('extracts specifiers from every import form', () => {
    // Built from fragments so this file's own source does not contain these specifiers.
    const I = 'imp' + 'ort';
    const E = 'exp' + 'ort';
    const src = [
      `${I} a from './a';`,
      `${I} type { B } from '../b';`,
      `${I} { c,\n d } from "three";`,
      `${E} * from './e';`,
      `const w = new Worker(new ${'URL'}('../workers/w.ts', ${I}.meta.url));`,
      `const m = await ${I}('./lazy');`,
      `${I} './side.css';`,
    ].join('\n');
    expect(specifiers(src).sort()).toEqual(['../b', '../workers/w.ts', './a', './e', './lazy', './side.css', 'three'].sort());
  });

  it('reaches nothing outside client/', () => {
    const files = listSources(ROOT);
    expect(files.length).toBeGreaterThan(20);
    const violations: string[] = [];
    const seen = new Set<string>();
    const queue = [...files];
    while (queue.length) {
      const file = queue.pop()!;
      if (seen.has(file)) continue;
      seen.add(file);
      const src = fs.readFileSync(file, 'utf8');
      for (const spec of specifiers(src)) {
        const rel = `${path.relative(ROOT, file)} → ${spec}`;
        if (spec.startsWith('.')) {
          if (spec.endsWith('.css')) {
            if (!path.resolve(path.dirname(file), spec).startsWith(ROOT + path.sep)) violations.push(rel);
            continue;
          }
          const target = resolveRelative(file, spec);
          if (!target) {
            violations.push(`${rel} (unresolved)`);
            continue;
          }
          const inside = target.startsWith(ROOT + path.sep);
          if (!inside) violations.push(`${rel} (outside client/: ${target})`);
          else queue.push(target);
        } else if (spec.startsWith('@cs/')) {
          const target = resolveRelative(path.join(ROOT, 'src', 'x.ts'), `./${spec.slice(4)}`);
          if (!target) violations.push(`${rel} (unresolved alias)`);
          else queue.push(target);
        } else if (spec.startsWith('@/') || spec.startsWith('/')) {
          violations.push(`${rel} (host-client alias or absolute path)`);
        } else if (!ALLOWED_PACKAGES.some((re) => re.test(spec))) {
          violations.push(`${rel} (package not on the Codescape allowlist)`);
        }
      }
    }
    expect(violations).toEqual([]);
  });

  it('walks the whole tree, including the build configs, and nothing in node_modules', () => {
    const rel = listSources(ROOT).map((f) => path.relative(ROOT, f));
    expect(rel).toContain('vite.config.ts');
    expect(rel).toContain(path.join('src', 'main.tsx'));
    expect(rel.filter((f) => f.split(path.sep).includes('node_modules'))).toEqual([]);
  });

  it('flags a relative import that escapes client/', () => {
    // Negative control: the walker must see the specifier and the path test must reject it.
    const escaping = path.resolve(ROOT, 'src', '..', '..', 'outside.ts');
    expect(escaping.startsWith(ROOT + path.sep)).toBe(false);
    expect(specifiers(`${'imp' + 'ort'} x from '../../outside';`)).toEqual(['../../outside']);
  });
});
