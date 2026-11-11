const fs = require('node:fs');
const path = require('node:path');
const { spawnSync } = require('node:child_process');
const root = path.resolve(__dirname, '..');
const binary = path.join(root, 'target/debug/smarter-diagrams');
const files = dir => fs.readdirSync(dir, { withFileTypes: true }).flatMap(e => e.name.startsWith('.') || e.name === 'public' ? [] : e.isDirectory() ? files(path.join(dir, e.name)) : [path.join(dir, e.name)]);
let diagrams = 0, snippets = 0, configurations = 0;
function checkDiagram(source, label) {
  const result = spawnSync(binary, ['-', '--format', 'check'], { input: source, encoding: 'utf8' });
  if (result.status !== 0) throw Error(`${label}: ${result.stderr || result.error}`);
  diagrams++;
}
const output = path.join(root, 'target/docs-snippets'); fs.mkdirSync(output, { recursive: true });
for (const file of files(path.join(root, 'site')).filter(file => file.endsWith('.md'))) {
  const text = fs.readFileSync(file, 'utf8');
  for (const match of text.matchAll(/^```(diagram|rust|toml|json)\s*\n([\s\S]*?)^```/gm)) {
    const source = match[2], label = `${path.relative(root, file)}:${text.slice(0, match.index).split('\n').length}`;
    if (match[1] === 'diagram') { checkDiagram(source, label); continue; }
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
    const snippet = path.join(output, `snippet_${snippets}.rs`); fs.writeFileSync(snippet, wrapped);
    const result = spawnSync('rustc', ['--edition=2024', '--crate-name', `docs_snippet_${snippets}`, '--emit=metadata', '-L', `dependency=${path.join(root, 'target/debug/deps')}`, '--extern', `smarter_diagrams=${path.join(root, 'target/debug/libsmarter_diagrams.rlib')}`, '--out-dir', output, snippet], { encoding: 'utf8' });
    if (result.status !== 0) throw Error(`${label}: ${result.stderr || result.error}`);
    snippets++;
  }
}
for (const file of fs.readdirSync(path.join(root, 'examples')).filter(file => file.endsWith('.diagram'))) checkDiagram(fs.readFileSync(path.join(root, 'examples', file), 'utf8'), file);
console.log(`Documentation: ${diagrams} diagram examples compiled; ${snippets} Rust snippets typechecked; ${configurations} TOML/JSON blocks parsed.`);
