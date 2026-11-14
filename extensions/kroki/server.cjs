'use strict';

const fs = require('node:fs');
const http = require('node:http');
const path = require('node:path');
const zlib = require('node:zlib');
const { execFile } = require('node:child_process');

const MAX_BODY_BYTES = 512 * 1024;
const RENDER_TIMEOUT_MS = 15000;

const FORMATS = {
  svg: 'image/svg+xml; charset=utf-8',
  png: 'image/png',
  pdf: 'application/pdf',
  gif: 'image/gif',
  apng: 'image/apng',
};

function defaultBinary() {
  if (process.env.SMARTER_DIAGRAMS_BIN) return process.env.SMARTER_DIAGRAMS_BIN;
  const root = path.resolve(__dirname, '../..');
  const exe = process.platform === 'win32' ? '.exe' : '';
  const debug = path.join(root, 'target', 'debug', `smarter-diagrams${exe}`);
  const release = path.join(root, 'target', 'release', `smarter-diagrams${exe}`);
  if (fs.existsSync(debug) && fs.existsSync(release)) {
    return fs.statSync(debug).mtimeMs >= fs.statSync(release).mtimeMs ? debug : release;
  }
  if (fs.existsSync(debug)) return debug;
  if (fs.existsSync(release)) return release;
  return 'smarter-diagrams';
}

function decodeKrokiPayload(encoded) {
  const normalized = encoded.replace(/-/g, '+').replace(/_/g, '/');
  const padded = normalized + '='.repeat((4 - (normalized.length % 4)) % 4);
  const raw = Buffer.from(padded, 'base64');
  try {
    return zlib.inflateSync(raw).toString('utf8');
  } catch {
    return zlib.inflateRawSync(raw).toString('utf8');
  }
}

function readBody(req) {
  return new Promise((resolve, reject) => {
    const chunks = [];
    let total = 0;
    req.on('data', chunk => {
      total += chunk.length;
      if (total > MAX_BODY_BYTES) {
        const err = new Error('Payload too large');
        err.statusCode = 413;
        req.destroy(err);
        reject(err);
        return;
      }
      chunks.push(chunk);
    });
    req.on('end', () => resolve(Buffer.concat(chunks).toString('utf8')));
    req.on('error', reject);
  });
}

function extractOptions(urlObj, headers, bodyOpts = {}) {
  const opts = {};
  const keys = ['theme', 'view', 'step'];
  for (const k of keys) {
    const h = headers[`kroki-diagram-options-${k}`];
    const q = urlObj.searchParams.get(k);
    const b = bodyOpts[k];
    const val = b ?? q ?? h;
    if (val !== undefined && val !== null && String(val).trim() !== '') {
      opts[k] = String(val).trim();
    }
  }
  return opts;
}

function renderDiagram(bin, source, format, options) {
  return new Promise((resolve, reject) => {
    const args = ['-', '--format', format];
    if (options.theme) args.push('--theme', options.theme);
    if (options.view) args.push('--view', options.view);
    if (options.step) args.push('--step', options.step);

    const child = execFile(
      bin,
      args,
      { encoding: 'buffer', maxBuffer: 16 * 1024 * 1024, timeout: RENDER_TIMEOUT_MS },
      (error, stdout, stderr) => {
        if (error) {
          const msg = stderr?.toString('utf8')?.trim() || error.message;
          const err = new Error(msg);
          err.statusCode = 400;
          reject(err);
          return;
        }
        resolve(stdout);
      }
    );
    child.stdin.end(source, 'utf8');
  });
}

function createKrokiServer({ bin = defaultBinary() } = {}) {
  return http.createServer(async (req, res) => {
    const urlObj = new URL(req.url, `http://${req.headers.host || '127.0.0.1'}`);
    const pathname = urlObj.pathname.replace(/\/+$/, '') || '/';
    const segments = pathname.split('/').filter(Boolean);

    try {
      if (req.method === 'GET' && pathname === '/health') {
        res.writeHead(200, { 'Content-Type': 'application/json; charset=utf-8' });
        res.end(JSON.stringify({ status: 'ok', engine: 'smarter-diagrams' }));
        return;
      }

      let format = null;
      let source = null;
      let options = null;

      if (req.method === 'GET') {
        if (segments.length === 3 && segments[0] === 'smarterdiagrams') {
          format = segments[1].toLowerCase();
          source = decodeKrokiPayload(segments[2]);
        } else if (segments.length === 2) {
          format = segments[0].toLowerCase();
          source = decodeKrokiPayload(segments[1]);
        } else {
          res.writeHead(404, { 'Content-Type': 'text/plain; charset=utf-8' });
          res.end('Not found');
          return;
        }
        options = extractOptions(urlObj, req.headers);
      } else if (req.method === 'POST') {
        const rawBody = await readBody(req);
        const contentType = (req.headers['content-type'] || '').toLowerCase();
        if (pathname === '/' || contentType.includes('application/json')) {
          let payload;
          try {
            payload = JSON.parse(rawBody || '{}');
          } catch {
            res.writeHead(400, { 'Content-Type': 'text/plain; charset=utf-8' });
            res.end('Invalid JSON request body');
            return;
          }
          if (payload.diagram_type && payload.diagram_type !== 'smarterdiagrams') {
            res.writeHead(400, { 'Content-Type': 'text/plain; charset=utf-8' });
            res.end(`Unsupported diagram_type: ${payload.diagram_type}`);
            return;
          }
          format = String(payload.output_format || (segments[1] || segments[0] || 'svg')).toLowerCase();
          source = payload.diagram_source;
          options = extractOptions(urlObj, req.headers, payload.diagram_options || {});
        } else if (segments.length === 2 && segments[0] === 'smarterdiagrams') {
          format = segments[1].toLowerCase();
          source = rawBody;
          options = extractOptions(urlObj, req.headers);
        } else if (segments.length === 1) {
          format = segments[0].toLowerCase();
          source = rawBody;
          options = extractOptions(urlObj, req.headers);
        } else {
          res.writeHead(404, { 'Content-Type': 'text/plain; charset=utf-8' });
          res.end('Not found');
          return;
        }
      } else {
        res.writeHead(405, { 'Content-Type': 'text/plain; charset=utf-8' });
        res.end('Method not allowed');
        return;
      }

      if (!format || !FORMATS[format]) {
        res.writeHead(400, { 'Content-Type': 'text/plain; charset=utf-8' });
        res.end(`Unsupported output format '${format}'; expected one of: ${Object.keys(FORMATS).join(', ')}`);
        return;
      }
      if (typeof source !== 'string' || !source.trim()) {
        res.writeHead(400, { 'Content-Type': 'text/plain; charset=utf-8' });
        res.end('Empty diagram source');
        return;
      }

      const output = await renderDiagram(bin, source, format, options);
      res.writeHead(200, {
        'Content-Type': FORMATS[format],
        'Cache-Control': req.method === 'GET' ? 'public, max-age=300' : 'no-store',
      });
      res.end(output);
    } catch (err) {
      const status = err.statusCode || 500;
      res.writeHead(status, { 'Content-Type': 'text/plain; charset=utf-8' });
      res.end(err.message || 'Internal error');
    }
  });
}

if (require.main === module) {
  const port = Number.parseInt(process.env.KROKI_PORT || '8008', 10);
  const host = process.env.KROKI_HOST || '0.0.0.0';
  const server = createKrokiServer();
  server.listen(port, host, () => {
    console.log(`smarter-diagrams kroki companion listening on http://${host}:${port}`);
  });
}

module.exports = { createKrokiServer, decodeKrokiPayload };
