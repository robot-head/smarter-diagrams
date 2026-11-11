// Loopback-only live preview transport for editors without an embedded browser.
const http = require('node:http');
const readline = require('node:readline');
const { spawn } = require('node:child_process');
const binary = process.argv[2] || 'smarter-diagrams';
let frame = { svg: '', error: '', revision: 0 }, generation = 0;
const html = `<!doctype html><meta charset="utf-8"><title>Diagram live preview</title><style>body{font-family:system-ui;margin:24px;background:#f8fafc}svg{max-width:100%;height:auto}pre{color:#a22}</style><main></main><pre role="status"></pre><script>let revision=-1;async function update(){const frame=await(await fetch('/frame')).json();if(frame.revision!==revision){revision=frame.revision;document.querySelector('main').innerHTML=frame.svg;document.querySelector('pre').textContent=frame.error;}}setInterval(()=>update().catch(()=>{}),200);update();</script>`;
const server = http.createServer((req, res) => {
  res.setHeader('Cache-Control', 'no-store');
  if (req.url === '/frame') { res.setHeader('Content-Type', 'application/json'); res.end(JSON.stringify(frame)); }
  else if (req.url === '/') { res.setHeader('Content-Type', 'text/html; charset=utf-8'); res.setHeader('Content-Security-Policy', "default-src 'self'; script-src 'unsafe-inline'; style-src 'unsafe-inline'"); res.end(html); }
  else { res.statusCode = 404; res.end(); }
});
server.listen(0, '127.0.0.1', () => console.log(JSON.stringify({ url: `http://127.0.0.1:${server.address().port}/` })));
readline.createInterface({ input: process.stdin }).on('line', line => {
  let source; try { source = JSON.parse(line).source; if (typeof source !== 'string') throw Error('source must be text'); }
  catch (error) { console.log(JSON.stringify({ error: error.message })); return; }
  const ticket = ++generation, child = spawn(binary, ['-', '--format', 'svg'], { shell: false });
  let svg = '', error = '';
  child.stdout.on('data', b => { svg += b; if (svg.length > 16000000) child.kill(); });
  child.stderr.on('data', b => { error += b; });
  child.on('error', e => { if (ticket === generation) { frame = { ...frame, error: e.message, revision: ticket }; console.log(JSON.stringify({ error: e.message })); } });
  child.on('close', code => { if (ticket !== generation) return; frame = { svg: code === 0 ? svg : frame.svg, error: code === 0 ? '' : error, revision: ticket }; console.log(JSON.stringify({ error: frame.error })); });
  child.stdin.on('error', () => {}); child.stdin.end(source);
}).on('close', () => server.close(() => process.exit(0)));
