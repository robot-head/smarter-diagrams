const fs = require('node:fs');
const path = require('node:path');
const { spawnSync } = require('node:child_process');
const root = path.resolve(__dirname, '..');
const binary = path.join(root, 'target/debug/smarter-diagrams');
const files = dir => fs.readdirSync(dir, { withFileTypes: true }).flatMap(e => e.name.startsWith('.') || e.name === 'public' ? [] : e.isDirectory() ? files(path.join(dir, e.name)) : [path.join(dir, e.name)]);
let diagrams = 0, snippets = 0, configurations = 0, diagnostics = 0;
function checkDiagram(source, label, expectError = false) {
  const result = spawnSync(binary, ['-', '--format', 'check'], { input: source, encoding: 'utf8' });
  if(expectError){if(result.status===0 || !result.stderr.includes('infeasible layout'))throw Error(`${label}: expected a layout diagnostic, got ${result.stderr || result.stdout}`);diagnostics++;return;}
  if (result.status !== 0) throw Error(`${label}: ${result.stderr || result.error}`);
  diagrams++;
}
if (!fs.existsSync(binary)) {
  const build = spawnSync('cargo', ['build', '--locked', '--all-features'], { cwd: root, stdio: 'inherit' });
  if (build.status !== 0) throw Error('Failed to build smarter-diagrams for docs check');
}
const output = path.join(root, 'target/docs-snippets');
const binDir = path.join(output, 'src/bin');
fs.rmSync(output, { recursive: true, force: true });
fs.mkdirSync(binDir, { recursive: true });
const rustSnippets = [];
for (const file of files(path.join(root, 'site')).filter(file => file.endsWith('.md'))) {
  const text = fs.readFileSync(file, 'utf8').replace(/\r\n/g, '\n');
  for (const match of text.matchAll(/^(`{3,})(diagram|rust|toml|json)([^\n]*)\n([\s\S]*?)^\1[ \t]*$/gm)) {
    const source = match[4], label = `${path.relative(root, file)}:${text.slice(0, match.index).split('\n').length}`;
    if (match[2] === 'diagram') { checkDiagram(source, label, /\bexpect-error\b/.test(match[3])); continue; }
    if (match[2] === 'toml') {
      const args = ['-c', 'import sys,tomllib; tomllib.loads(sys.stdin.read())'];
      const opts = { input: source, encoding: 'utf8' };
      let result = spawnSync('python3', args, opts);
      if (result.error || result.status !== 0) {
        const fallback = spawnSync('python', args, opts);
        if (!fallback.error || result.error) result = fallback;
      }
      if (result.error || result.status !== 0) throw Error(`${label}: ${result.stderr || result.error}`);
      configurations++; continue;
    }
    if (match[2] === 'json') {
      try { JSON.parse(source); } catch {
        try { for (const line of source.trim().split('\n')) JSON.parse(line); }
        catch (error) { throw Error(`${label}: ${error.message}`); }
      }
      configurations++; continue;
    }
    const wrapped = /^fn main\(/m.test(source) ? source : `fn main() -> Result<(), Box<dyn std::error::Error>> {\n${source}\nOk(())\n}`;
    const snippetPath = path.join(binDir, `snippet_${snippets}.rs`);
    fs.writeFileSync(snippetPath, wrapped);
    rustSnippets.push({ name: `snippet_${snippets}`, label });
    snippets++;
  }
}
const decodeHtml = s => s.replace(/&lt;/g, '<').replace(/&gt;/g, '>').replace(/&amp;/g, '&').replace(/&quot;/g, '"').replace(/&#39;/g, "'");
const indexHtml = fs.readFileSync(path.join(root, 'site/public/index.html'), 'utf8');
for (const banned of ['data-ui=', 'data-ui-load', 'ui-win', 'ui-pipe', 'ui-term', 'assets/diagrams/mount.js', 'sd check', '.dgm']) {
  if (indexHtml.includes(banned)) throw Error(`site/public/index.html must not contain fabricated UI or invalid reference: ${banned}`);
}
let homeDiagrams = 0, homeRust = 0;
for (const match of indexHtml.matchAll(/<(code|textarea)\b[^>]*\bdata-diagram-source\b[^>]*>([\s\S]*?)<\/\1>/gi)) {
  checkDiagram(`${decodeHtml(match[2]).trim()}\n`, `site/public/index.html#${homeDiagrams + 1}`);
  homeDiagrams++;
}
if (homeDiagrams < 5) throw Error(`site/public/index.html must embed at least 5 compiled data-diagram-source blocks, found ${homeDiagrams}`);
for (const match of indexHtml.matchAll(/<code\b[^>]*\bdata-rust-source\b[^>]*>([\s\S]*?)<\/code>/gi)) {
  const source = `${decodeHtml(match[1]).trim()}\n`;
  const wrapped = /^fn main\(/m.test(source) ? source : `fn main() -> Result<(), Box<dyn std::error::Error>> {\n${source}\nOk(())\n}`;
  const snippetPath = path.join(binDir, `snippet_${snippets}.rs`);
  fs.writeFileSync(snippetPath, wrapped);
  rustSnippets.push({ name: `snippet_${snippets}`, label: `site/public/index.html#rust-${homeRust + 1}` });
  snippets++;
  homeRust++;
}
if (homeRust < 1) throw Error('site/public/index.html must embed at least 1 typechecked data-rust-source snippet');
if (rustSnippets.length > 0) {
  fs.writeFileSync(
    path.join(output, 'Cargo.toml'),
    `[package]\nname = "docs-snippets"\nversion = "0.1.0"\nedition = "2024"\n\n[workspace]\n\n[dependencies]\nsmarter-diagrams = { path = "../..", features = ["export", "code-bindings", "verification", "wasm", "wasm-runtime"] }\n`
  );
  fs.copyFileSync(path.join(root, 'Cargo.lock'), path.join(output, 'Cargo.lock'));
  const result = spawnSync(
    'cargo',
    ['check', '--offline', '--manifest-path', path.join(output, 'Cargo.toml'), '--target-dir', path.join(root, 'target'), '--bins'],
    { cwd: root, encoding: 'utf8' }
  );
  if (result.status !== 0) {
    const labels = rustSnippets.map(s => `${s.name}: ${s.label}`).join('\n');
    throw Error(`Rust snippet typecheck failed:\n${result.stderr || result.error}\nSnippets:\n${labels}`);
  }
}
for (const file of fs.readdirSync(path.join(root, 'examples')).filter(file => file.endsWith('.diagram'))) checkDiagram(fs.readFileSync(path.join(root, 'examples', file), 'utf8'), file);
console.log(`Documentation: ${diagrams} diagram examples compiled; ${diagnostics} expected layout diagnostics; ${snippets} Rust snippets typechecked; ${configurations} TOML/JSON blocks parsed.`);
