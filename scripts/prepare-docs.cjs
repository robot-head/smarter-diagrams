const fs = require('node:fs');
const path = require('node:path');
const { spawnSync } = require('node:child_process');
const root = path.resolve(__dirname, '..');
const publicDir = path.join(root, 'site/public');
const release = process.argv.includes('--release');
const copy = (from, to) => { fs.mkdirSync(path.dirname(to), { recursive: true }); fs.cpSync(from, to, { recursive: true }); };
const required = ['web/pkg/smarter_diagrams_bg.wasm', 'web/pkg-runtime/smarter_diagrams_bg.wasm', 'target/doc/smarter_diagrams/index.html'];
if (release) for (const file of required) if (!fs.existsSync(path.join(root, file))) throw Error(`Missing ${file}. Build both Wasm bundles and run cargo doc --locked --all-features --no-deps first.`);
for (const dir of ['demo', 'api', 'evidence', 'examples', 'generated']) fs.rmSync(path.join(publicDir, dir), { recursive: true, force: true });
copy(path.join(root, 'web'), path.join(publicDir, 'demo'));
if (fs.existsSync(path.join(root, 'target/doc'))) copy(path.join(root, 'target/doc'), path.join(publicDir, 'api'));
for (const file of ['benchmarks.csv', 'browser-benchmark.json', 'wasm-size.json', 'github-plan.json']) copy(path.join(root, 'docs', file), path.join(publicDir, 'evidence', file));
copy(path.join(root, 'docs/requirements'), path.join(publicDir, 'evidence/requirements'));
fs.mkdirSync(path.join(root, 'site/reference'), { recursive: true });
for (const file of ['architecture', 'performance', 'verification']) {
  let text = fs.readFileSync(path.join(root, 'docs', `${file}.md`), 'utf8');
  // The originals remain canonical. Rewrite repository-relative evidence links
  // for the hosted guide without maintaining a second copy of their content.
  text = text.replace(/\]\(([^)]+)\)/g, (match, target) => {
    if (/^(https?:|#)/.test(target)) return match;
    if (target === 'requirements/') return '](/evidence/requirements/RFC-001.txt)';
    if (target.startsWith('../')) return `](https://github.com/robot-head/smarter-diagrams/blob/main/${target.slice(3)})`;
    if (target.startsWith('requirements/')) return `](/evidence/${target})`;
    if (target.endsWith('.md')) return `](/reference/${target.replace(/\.md$/, '')})`;
    return `](/evidence/${target})`;
  });
  fs.writeFileSync(path.join(root, 'site/reference', `${file}.md`), `---\neditLink: false\n---\n\n${text}`);
}
for (const file of fs.readdirSync(path.join(root, 'examples')).filter(file => file.endsWith('.diagram'))) {
  copy(path.join(root, 'examples', file), path.join(publicDir, 'examples', file));
  copy(path.join(root, 'examples', file), path.join(publicDir, 'demo/examples', file));
  const result = spawnSync(path.join(root, 'target/debug/smarter-diagrams'), [path.join(root, 'examples', file)], { encoding: 'utf8' });
  if (result.status !== 0) throw Error(`Cannot render ${file}: ${result.stderr || result.error}`);
  const output = path.join(publicDir, 'generated', file.replace(/\.diagram$/, '.svg'));
  fs.mkdirSync(path.dirname(output), { recursive: true }); fs.writeFileSync(output, result.stdout);
}
console.log('Prepared documentation, compiled examples, Wasm demos and Rust API.');
