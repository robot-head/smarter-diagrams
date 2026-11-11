const fs = require('node:fs');
const path = require('node:path');
const { spawnSync } = require('node:child_process');
const { createHash } = require('node:crypto');
const { samples, key } = require('../site/.vitepress/diagram-samples.cjs');
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
const languageSamples = new Map();
const comparisons = [];
for (const file of fs.readdirSync(path.join(root, 'site/language')).filter(file => file.endsWith('.md'))) {
  const text = fs.readFileSync(path.join(root, 'site/language', file), 'utf8');
  for (const match of text.matchAll(/^```(diagram[^\n]*)\n([\s\S]*?)^```/gm)) {
    const group=samples(match[2],match[1]);
    for (const sample of group) languageSamples.set(key(sample), { ...sample, label: `${file}: ${sample.label}` });
    if(group.length>1)comparisons.push(group.map(key));
  }
}
for (const file of fs.readdirSync(path.join(root, 'examples')).filter(file => file.endsWith('.diagram'))) {
  copy(path.join(root, 'examples', file), path.join(publicDir, 'examples', file));
  copy(path.join(root, 'examples', file), path.join(publicDir, 'demo/examples', file));
  const result = spawnSync(path.join(root, 'target/debug/smarter-diagrams'), [path.join(root, 'examples', file)], { encoding: 'utf8' });
  if (result.status !== 0) throw Error(`Cannot render ${file}: ${result.stderr || result.error}`);
  const output = path.join(publicDir, 'generated', file.replace(/\.diagram$/, '.svg'));
  fs.mkdirSync(path.dirname(output), { recursive: true }); fs.writeFileSync(output, result.stdout);
  // Imported example fences use exactly the same rendering path as inline code.
  const sample = samples(fs.readFileSync(path.join(root, 'examples', file), 'utf8'))[0];
  languageSamples.set(key(sample), { ...sample, label: file });
}
const build = spawnSync('cargo', ['build', '--locked', '--all-features', '--example', 'render_docs'], { cwd: root, encoding: 'utf8' });
if (build.status !== 0) throw Error(build.stderr || build.error);
const requests = [...languageSamples.values()];
const rendered = spawnSync(path.join(root, 'target/debug/examples/render_docs'), [], { input: JSON.stringify(requests), encoding: 'utf8', maxBuffer: 16 * 1024 * 1024 });
if (rendered.status !== 0) throw Error(rendered.stderr || rendered.error);
const manifest = {}, images = JSON.parse(rendered.stdout);
const results=new Map(requests.map((request,i)=>[key(request),images[i]]));
for(const group of comparisons){
  const boxes=group.map(id=>results.get(id).viewBox);
  const left=Math.min(...boxes.map(b=>b[0])),top=Math.min(...boxes.map(b=>b[1]));
  const right=Math.max(...boxes.map(b=>b[0]+b[2])),bottom=Math.max(...boxes.map(b=>b[1]+b[3]));
  for(const id of group){const result=results.get(id);result.viewBox=[left,top,right-left,bottom-top];result.svg=result.svg.replace(/viewBox="[^"]+" width="[^"]+" height="[^"]+"/,`viewBox="${result.viewBox.join(' ')}" width="${right-left}" height="${bottom-top}"`);}
}
const destination = path.join(publicDir, 'generated/language'); fs.mkdirSync(destination, { recursive: true });
for (let i = 0; i < requests.length; i++) {
  const result=images[i];
  if(result.diagnostic){manifest[key(requests[i])]=result;continue;}
  const {svg,...metadata}=result;
  const file = `${createHash('sha256').update(svg).digest('hex')}.svg`;
  fs.writeFileSync(path.join(destination, file), svg); manifest[key(requests[i])] = {file,...metadata};
}
fs.writeFileSync(path.join(destination, 'manifest.json'), JSON.stringify(manifest));
console.log(`Prepared documentation, examples, Wasm, Rust API and ${requests.length} language diagram renders.`);
