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
  const text = fs.readFileSync(file, 'utf8');
  for (const match of text.matchAll(/^```(diagram|rust|toml|json)([^\n]*)\n([\s\S]*?)^```/gm)) {
    const source = match[3], label = `${path.relative(root, file)}:${text.slice(0, match.index).split('\n').length}`;
    if (match[1] === 'diagram') { checkDiagram(source, label, /\bexpect-error\b/.test(match[2])); continue; }
    if (match[1] === 'toml') {
      const result = spawnSync('python3', ['-c', 'import sys,tomllib; tomllib.loads(sys.stdin.read())'], { input: source, encoding: 'utf8' });
      if (result.status !== 0) throw Error(`${label}: ${result.stderr || result.error}`);
      configurations++; continue;
    }
    if (match[1] === 'json') {
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
