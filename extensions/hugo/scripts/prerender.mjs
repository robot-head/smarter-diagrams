#!/usr/bin/env node
// Pre-renders fenced ```diagram and ```smarter-diagrams blocks in a Hugo content
// tree into deterministic assets/smarter-diagrams/<sha256>.svg resources, and
// optionally copies the WebAssembly bundle into static/smarter-diagrams/pkg/.

import fs from 'node:fs';
import path from 'node:path';
import { createHash } from 'node:crypto';
import { spawnSync } from 'node:child_process';
import { pathToFileURL } from 'node:url';

function parseArgs(argv) {
  const opts = {
    site: process.cwd(),
    content: null,
    out: null,
    bin: process.env.SMARTER_DIAGRAMS_BIN || 'smarter-diagrams',
    wasmSrc: null,
    copyWasm: null,
    force: false,
  };
  for (let i = 0; i < argv.length; i++) {
    const arg = argv[i];
    if (arg === '--site') opts.site = path.resolve(argv[++i]);
    else if (arg === '--content') opts.content = path.resolve(argv[++i]);
    else if (arg === '--out') opts.out = path.resolve(argv[++i]);
    else if (arg === '--bin') opts.bin = argv[++i];
    else if (arg === '--wasm-src') opts.wasmSrc = path.resolve(argv[++i]);
    else if (arg === '--copy-wasm') opts.copyWasm = argv[i + 1] && !argv[i + 1].startsWith('--') ? path.resolve(argv[++i]) : 'default';
    else if (arg === '--force') opts.force = true;
    else if (arg === '-h' || arg === '--help') {
      console.log(
        'Usage: node prerender.mjs [--site DIR] [--content DIR] [--out DIR] [--bin PATH] [--wasm-src DIR] [--copy-wasm [DIR]] [--force]'
      );
      process.exit(0);
    } else {
      throw new Error(`Unknown argument: ${arg}`);
    }
  }
  opts.content = opts.content || path.join(opts.site, 'content');
  opts.out = opts.out || path.join(opts.site, 'assets', 'smarter-diagrams');
  if (opts.copyWasm === 'default') {
    opts.copyWasm = path.join(opts.site, 'static', 'smarter-diagrams', 'pkg');
  }
  return opts;
}

function walkMarkdown(dir, out = []) {
  if (!fs.existsSync(dir)) return out;
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    const full = path.join(dir, entry.name);
    if (entry.isDirectory()) walkMarkdown(full, out);
    else if (entry.isFile() && /\.(md|markdown|mdown)$/i.test(entry.name)) out.push(full);
  }
  return out;
}

export function parseHugoAttrs(infoRest) {
  const attrs = {};
  const braceMatch = infoRest.match(/\{([\s\S]*)\}/);
  if (!braceMatch) return attrs;
  const body = braceMatch[1];
  const re = /([A-Za-z_][A-Za-z0-9_-]*)\s*=\s*(?:"((?:[^"\\]|\\.)*)"|'((?:[^'\\]|\\.)*)'|([^\s}]+))/g;
  for (const m of body.matchAll(re)) {
    const rawVal = m[2] ?? m[3] ?? m[4] ?? '';
    attrs[m[1]] = rawVal.replace(/\\(["'\\])/g, '$1');
  }
  return attrs;
}

export function extractFences(markdown) {
  const normalized = markdown.replace(/\r\n/g, '\n');
  const lines = normalized.split('\n');
  const blocks = [];
  let i = 0;
  while (i < lines.length) {
    const open = lines[i].match(/^\s*(`{3,}|~{3,})\s*(diagram|smarter-diagrams)(?=\s|\{|$)(.*)$/);
    if (!open) {
      i++;
      continue;
    }
    const fence = open[1];
    const fenceChar = fence[0];
    const attrs = parseHugoAttrs(open[3] || '');
    const body = [];
    i++;
    const closeRe = new RegExp(`^\\s*${fenceChar}{${fence.length},}\\s*$`);
    while (i < lines.length && !closeRe.test(lines[i])) {
      body.push(lines[i]);
      i++;
    }
    if (i < lines.length) i++;
    blocks.push({ source: body.join('\n').trim(), attrs });
  }
  return blocks;
}

export function computeHash({ source, view = '', focus = '', hops = '' }) {
  const effectiveHops = String(hops) !== '' ? String(hops) : focus ? '1' : '';
  const keyInput = `smarter-diagrams:v1\nview=${view}\nfocus=${focus}\nhops=${effectiveHops}\n---\n${source}`;
  return createHash('sha256').update(keyInput, 'utf8').digest('hex');
}

function responsiveSvg(svg) {
  return svg.replace(/width="[^"]+" height="[^"]+" role="img"/, 'width="100%" height="auto" role="img"');
}

async function loadWasmRenderer(wasmSrc) {
  if (!wasmSrc) return null;
  const jsPath = path.join(wasmSrc, 'smarter_diagrams.js');
  const bgPath = path.join(wasmSrc, 'smarter_diagrams_bg.wasm');
  if (!fs.existsSync(jsPath) || !fs.existsSync(bgPath)) return null;
  const mod = await import(pathToFileURL(jsPath).href);
  mod.initSync({ module: fs.readFileSync(bgPath) });
  return mod;
}

async function main() {
  const opts = parseArgs(process.argv.slice(2));
  if (opts.copyWasm) {
    if (!opts.wasmSrc) {
      throw new Error('--copy-wasm requires --wasm-src pointing to web/pkg');
    }
    fs.mkdirSync(opts.copyWasm, { recursive: true });
    for (const name of ['smarter_diagrams.js', 'smarter_diagrams_bg.wasm']) {
      fs.copyFileSync(path.join(opts.wasmSrc, name), path.join(opts.copyWasm, name));
    }
  }

  const wasm = await loadWasmRenderer(opts.wasmSrc);
  const files = walkMarkdown(opts.content);
  fs.mkdirSync(opts.out, { recursive: true });

  let total = 0;
  let rendered = 0;
  let cached = 0;

  for (const file of files) {
    const blocks = extractFences(fs.readFileSync(file, 'utf8'));
    for (const block of blocks) {
      if ((block.attrs.mode || 'auto') === 'wasm') continue;
      const view = block.attrs.view || '';
      const focus = block.attrs.focus || '';
      const hops = block.attrs.hops || (focus ? '1' : '');
      const hash = computeHash({ source: block.source, view, focus, hops });
      const target = path.join(opts.out, `${hash}.svg`);
      total++;
      if (!opts.force && fs.existsSync(target)) {
        cached++;
        continue;
      }

      let svg;
      if (focus) {
        if (!wasm) {
          throw new Error(
            `${file}: focus="${focus}" static pre-rendering requires --wasm-src pointing to web/pkg (or set mode="wasm")`
          );
        }
        if (view) {
          throw new Error(`${file}: cannot combine view and focus in the same Hugo codeblock`);
        }
        const doc = new wasm.WasmDocument(block.source);
        svg = doc.focus_svg(focus, Math.max(1, Number.parseInt(hops || '1', 10) || 1));
        doc.free();
      } else {
        const cliArgs = ['-'];
        if (view) cliArgs.push('--view', view);
        const res = spawnSync(opts.bin, cliArgs, { input: block.source, encoding: 'utf8' });
        if (res.status === 0) {
          svg = res.stdout;
        } else if (res.error && wasm && !view) {
          svg = wasm.render_svg(block.source);
        } else {
          throw new Error(
            `Failed to compile diagram in ${file}${view ? ` (view=${view})` : ''}:\n${res.stderr || res.error}`
          );
        }
      }
      fs.writeFileSync(target, responsiveSvg(svg), 'utf8');
      rendered++;
    }
  }

  console.log(
    `smarter-diagrams hugo prerender: ${total} diagram block(s) across ${files.length} file(s) (${rendered} rendered, ${cached} cached).`
  );
}

if (import.meta.url === pathToFileURL(process.argv[1]).href) {
  main().catch(err => {
    console.error(err.message || err);
    process.exit(1);
  });
}
