#!/usr/bin/env node
// Benchmarks Outernet Council nmtscli + Graphviz dot against smarter-diagrams
// on real nmts/v1/examples fragments and synthetic scaling fragments.
'use strict';

const fs = require('node:fs');
const path = require('node:path');
const { spawnSync } = require('node:child_process');

const ROOT = path.resolve(__dirname, '..');
const SCRATCH = process.env.NMTS_BENCH_SCRATCH
  || 'C:\\Users\\Matt Stone\\.gemini\\antigravity\\brain\\72d066c0-c07d-4934-9ea6-8f3b1a1f941e\\scratch';
const TOOLS = path.join(SCRATCH, 'tools');
const NMTS_REPO = path.join(SCRATCH, 'nmts');
const WORK_DIR = path.join(SCRATCH, 'nmts_bench');

const NMTSCLI = process.env.NMTSCLI_BIN || path.join(TOOLS, 'bin', 'nmtscli.exe');
const TEMPLATE2TXTPB = process.env.TEMPLATE2TXTPB_BIN || path.join(TOOLS, 'bin', 'template2txtpb.exe');
const DOT_BIN = process.env.DOT_BIN || path.join(TOOLS, 'graphviz', 'bin', 'dot.exe');
const SD_BIN = process.env.SD_BIN || path.join(ROOT, 'target', 'release', 'smarter-diagrams.exe');

const TIMEOUT_MS = Number(process.env.NMTS_BENCH_TIMEOUT_MS || 35000);
const REPEATS_SMALL = 3;

function stripCommentsAndFindBlocks(raw) {
  // Strip '#' comments outside quotes and iterate top-level entity/relationship blocks cleanly.
  let clean = '';
  let inStr = false;
  let escaped = false;
  for (let i = 0; i < raw.length; i++) {
    const ch = raw[i];
    if (inStr) {
      clean += ch;
      if (escaped) {
        escaped = false;
      } else if (ch === '\\') {
        escaped = true;
      } else if (ch === '"') {
        inStr = false;
      }
    } else if (ch === '"') {
      inStr = true;
      clean += ch;
    } else if (ch === '#') {
      while (i < raw.length && raw[i] !== '\n') i++;
      if (i < raw.length) clean += '\n';
    } else {
      clean += ch;
    }
  }

  const blocks = [];
  const headerRe = /\b(entity|relationship)\s*:?\s*\{/g;
  let m;
  while ((m = headerRe.exec(clean)) !== null) {
    const kind = m[1];
    let pos = headerRe.lastIndex;
    let depth = 1;
    let str = false;
    let esc = false;
    while (pos < clean.length && depth > 0) {
      const c = clean[pos++];
      if (str) {
        if (esc) esc = false;
        else if (c === '\\') esc = true;
        else if (c === '"') str = false;
      } else if (c === '"') {
        str = true;
      } else if (c === '{') {
        depth++;
      } else if (c === '}') {
        depth--;
      }
    }
    if (depth === 0) {
      blocks.push({ kind, body: clean.slice(headerRe.lastIndex, pos - 1) });
      headerRe.lastIndex = pos;
    }
  }
  return blocks;
}

function parseTxtpbFragments(files) {
  const entities = new Map();
  const relationships = [];
  for (const file of files) {
    const raw = fs.readFileSync(file, 'utf8');
    for (const block of stripCommentsAndFindBlocks(raw)) {
      if (block.kind === 'entity') {
        const idM = block.body.match(/\bid\s*:\s*"([^"]+)"/);
        if (!idM) continue;
        const ekM = block.body.match(/\b(ek_[a-z0-9_]+)\s*:?\s*\{/);
        const dispLabelM = block.body.match(/key\s*:\s*"display_name"\s+value\s*:\s*"([^"]+)"/);
        const nameM = block.body.match(/\b(?:name|display_name)\s*:\s*"([^"]+)"/);
        entities.set(idM[1], {
          id: idM[1],
          kind: ekM ? ekM[1].toUpperCase() : 'EK_ENTITY',
          label: dispLabelM ? dispLabelM[1] : nameM ? nameM[1] : '',
        });
      } else if (block.kind === 'relationship') {
        const kM = block.body.match(/\bkind\s*:\s*(RK_[A-Z0-9_]+)/);
        const aM = block.body.match(/\ba\s*:\s*"([^"]+)"/);
        const zM = block.body.match(/\bz\s*:\s*"([^"]+)"/);
        if (kM && aM && zM) {
          relationships.push({ kind: kM[1], a: aM[1], z: zM[1] });
        }
      }
    }
  }
  return { entities, relationships };
}

function sanitizeId(raw, used) {
  let base = raw.toLowerCase().replace(/[^a-z0-9]+/g, '_').replace(/^_+|_+$/g, '');
  if (!base || /^[0-9]/.test(base)) base = 'n_' + base;
  if (!used.has(base)) {
    used.add(base);
    return base;
  }
  let idx = 2;
  while (used.has(`${base}_${idx}`)) idx += 1;
  const candidate = `${base}_${idx}`;
  used.add(candidate);
  return candidate;
}

function lowerNmtsToDiagram(files) {
  const t0 = performance.now();
  const { entities, relationships } = parseTxtpbFragments(files);
  const children = new Map();
  const hasParent = new Set();
  const links = [];

  for (const r of relationships) {
    if (!entities.has(r.a) || !entities.has(r.z)) continue;
    if (r.kind === 'RK_CONTAINS') {
      if (hasParent.has(r.z) || r.a === r.z) continue;
      hasParent.add(r.z);
      if (!children.has(r.a)) children.set(r.a, []);
      children.get(r.a).push(r.z);
    } else {
      links.push(r);
    }
  }

  // Break any accidental contains cycles safely and assign unique scoped IDs.
  const localIds = new Map();
  const qualifiedIds = new Map();
  function assign(entityId, scopePrefix, scopeUsed, visiting) {
    if (visiting.has(entityId) || qualifiedIds.has(entityId)) return;
    visiting.add(entityId);
    const local = sanitizeId(entityId.replace(/^uuid8\(|\)$/g, ''), scopeUsed);
    const qid = scopePrefix ? `${scopePrefix}.${local}` : local;
    localIds.set(entityId, local);
    qualifiedIds.set(entityId, qid);
    const kids = (children.get(entityId) || []).slice().sort();
    const kidUsed = new Set();
    for (const k of kids) assign(k, qid, kidUsed, visiting);
  }

  const rootIds = [...entities.keys()].filter(id => !hasParent.has(id)).sort();
  const rootUsed = new Set();
  for (const r of rootIds) assign(r, '', rootUsed, new Set());
  for (const id of [...entities.keys()].sort()) {
    if (!qualifiedIds.has(id)) assign(id, '', rootUsed, new Set());
  }

  const lines = [];
  function shapeAttr(kind) {
    if (kind === 'EK_PHYSICAL_MEDIUM_LINK' || kind === 'EK_LOGICAL_PACKET_LINK') return ' [shape: queue]';
    if (kind === 'EK_SDN_AGENT') return ' [shape: cache]';
    return '';
  }
  function shortLabel(ent) {
    const clean = (ent.label || ent.id.replace(/^uuid8\(|\)$/g, '')).replace(/"/g, "'");
    return `${clean}\\n(${ent.kind})`;
  }
  function emitEntity(entityId, indent) {
    const ent = entities.get(entityId);
    const local = localIds.get(entityId);
    const kids = (children.get(entityId) || []).filter(k => qualifiedIds.has(k));
    const pad = '  '.repeat(indent);
    if (kids.length === 0) {
      lines.push(`${pad}${local}: "${shortLabel(ent)}"${shapeAttr(ent.kind)}`);
      return;
    }
    lines.push(`${pad}${local}: "${shortLabel(ent)}" {`);
    for (const k of kids) emitEntity(k, indent + 1);
    lines.push(`${pad}}`);
  }

  for (const r of rootIds) emitEntity(r, 0);
  const emittedPairs = new Set();
  for (const r of links) {
    const a = qualifiedIds.get(r.a);
    const z = qualifiedIds.get(r.z);
    if (!a || !z || a === z) continue;
    const key = `${a}->${z}|${r.kind}`;
    if (emittedPairs.has(key)) continue;
    emittedPairs.add(key);
    lines.push(`${a} -> ${z}: "${r.kind}"`);
  }

  const source = lines.join('\n') + '\n';
  return {
    source,
    entityCount: entities.size,
    edgeCount: emittedPairs.size,
    lowerMs: performance.now() - t0,
  };
}

function generateSyntheticFragment(platforms) {
  const dir = path.join(WORK_DIR, `synth_p${platforms}`);
  fs.mkdirSync(dir, { recursive: true });
  const file = path.join(dir, 'constellation.txtpb');
  const out = [];
  for (let i = 0; i < platforms; i++) {
    const p = `p${String(i).padStart(4, '0')}`;
    // Lexicographical order pXXXX_0_platform < pXXXX_1_* satisfies nmtscli is_root(E) check (E @< C)
    const plat = `${p}_0_platform`;
    const sdn = `${p}_1_sdn`;
    const router = `${p}_2_router`;
    const iface = `${p}_3_iface`;
    const mod = `${p}_4_mod`;
    const ant = `${p}_5_ant`;
    const port = `${p}_6_port`;
    const link = `${p}_7_link`;

    out.push(`entity { id: "${plat}" ek_platform {} }`);
    out.push(`entity { id: "${sdn}" ek_sdn_agent {} }`);
    out.push(`entity { id: "${router}" ek_network_node {} }`);
    out.push(`entity { id: "${iface}" ek_interface {} }`);
    out.push(`entity { id: "${mod}" ek_modulator {} }`);
    out.push(`entity { id: "${ant}" ek_antenna {} }`);
    out.push(`entity { id: "${port}" ek_port { name: "eth0" } }`);
    out.push(`entity { id: "${link}" ek_physical_medium_link {} }`);

    for (const c of [sdn, router, iface, mod, ant, port]) {
      out.push(`relationship { kind: RK_CONTAINS a: "${plat}" z: "${c}" }`);
    }
    out.push(`relationship { kind: RK_CONTROLS a: "${sdn}" z: "${mod}" }`);
    out.push(`relationship { kind: RK_TRAVERSES a: "${router}" z: "${iface}" }`);
    out.push(`relationship { kind: RK_TRAVERSES a: "${iface}" z: "${port}" }`);
    out.push(`relationship { kind: RK_SIGNAL_TRANSITS a: "${mod}" z: "${ant}" }`);
  }
  for (let i = 0; i < platforms; i++) {
    const next = (i + 1) % platforms;
    const p = `p${String(i).padStart(4, '0')}`;
    const q = `p${String(next).padStart(4, '0')}`;
    if (platforms > 1) {
      out.push(`relationship { kind: RK_SIGNAL_TRANSITS a: "${p}_5_ant" z: "${p}_7_link" }`);
      out.push(`relationship { kind: RK_SIGNAL_TRANSITS a: "${p}_7_link" z: "${q}_5_ant" }`);
    }
  }
  fs.writeFileSync(file, out.join('\n') + '\n');
  return [file];
}

function runTimed(bin, args, opts = {}) {
  const t0 = performance.now();
  const res = spawnSync(bin, args, {
    input: opts.input,
    encoding: 'utf8',
    timeout: TIMEOUT_MS,
    maxBuffer: 128 * 1024 * 1024,
  });
  const ms = performance.now() - t0;
  if (res.error) {
    return {
      ok: false,
      ms,
      reason: res.error.code === 'ETIMEDOUT' ? `timeout > ${TIMEOUT_MS}ms` : res.error.message,
      stdout: res.stdout || '',
    };
  }
  if (res.status !== 0) {
    return {
      ok: false,
      ms,
      reason: (res.stderr || `exit ${res.status}`).trim().slice(0, 160),
      stdout: res.stdout || '',
    };
  }
  return { ok: true, ms, stdout: res.stdout || '' };
}

function medianTime(fn, allowRepeats) {
  const first = fn();
  if (!first.ok) return first;
  if (!allowRepeats || first.ms > 1000) return first;
  const times = [first.ms];
  let latest = first;
  for (let r = 1; r < REPEATS_SMALL; r++) {
    latest = fn();
    if (!latest.ok) return latest;
    times.push(latest.ms);
  }
  times.sort((a, b) => a - b);
  return { ...latest, ms: times[Math.floor(times.length / 2)] };
}

function parseSvgDims(svgText) {
  if (!svgText) return null;
  const vb = svgText.match(/viewBox="([\d.\-]+)\s+([\d.\-]+)\s+([\d.\-]+)\s+([\d.\-]+)"/);
  if (vb) {
    const w = Number(vb[3]);
    const h = Number(vb[4]);
    return { width: Math.round(w), height: Math.round(h), aspectRatio: Number((Math.max(w, h) / Math.max(1, Math.min(w, h))).toFixed(2)) };
  }
  const wh = svgText.match(/width="([\d.]+)(?:pt|px)?"\s+height="([\d.]+)(?:pt|px)?"/);
  if (wh) {
    const w = Number(wh[1]);
    const h = Number(wh[2]);
    return { width: Math.round(w), height: Math.round(h), aspectRatio: Number((Math.max(w, h) / Math.max(1, Math.min(w, h))).toFixed(2)) };
  }
  return null;
}

function ensureSatelliteMefExpanded() {
  const mDir = path.join(NMTS_REPO, 'v1', 'examples', 'satellite_mef');
  const outDir = path.join(WORK_DIR, 'satellite_mef');
  fs.mkdirSync(outDir, { recursive: true });
  const specs = [
    {
      tmpls: [path.join(mDir, 'geo_satellites', 'geo_satellite_gen3.txtpb.tmpl'), path.join(mDir, 'geo_satellites', 'block_ek_interface.txtpb.tmpl')],
      input: path.join(mDir, 'geo_satellites', 'GEO_1.json'),
      out: path.join(outDir, 'GEO_1.txtpb'),
    },
    {
      tmpls: [path.join(mDir, 'ground_stations', 'gs.txtpb.tmpl')],
      input: path.join(mDir, 'ground_stations', 'arlington_virginia.json'),
      out: path.join(outDir, 'arlington_virginia.txtpb'),
    },
    {
      tmpls: [path.join(mDir, 'pops', 'pop.txtpb.tmpl')],
      input: path.join(mDir, 'pops', 'us_east1.json'),
      out: path.join(outDir, 'us_east1.txtpb'),
    },
    {
      tmpls: [path.join(mDir, 'user_terminals', 'user_terminal_gen1.txtpb.tmpl')],
      input: path.join(mDir, 'user_terminals', 'US_CA-0_0.json'),
      out: path.join(outDir, 'US_CA-0_0.txtpb'),
    },
  ];
  for (const s of specs) {
    if (!fs.existsSync(s.out)) {
      const args = s.tmpls.flatMap(t => ['-tmpl_filename', t]).concat(['-input', s.input, '-output', s.out]);
      const res = runTimed(TEMPLATE2TXTPB, args);
      if (!res.ok) throw new Error(`template2txtpb failed: ${res.reason}`);
    }
  }
  const sdnOut = path.join(outDir, 'satellite_nms.txtpb');
  fs.copyFileSync(path.join(mDir, 'sdn', 'satellite_nms.txtpb'), sdnOut);
  return fs.readdirSync(outDir).filter(f => f.endsWith('.txtpb')).sort().map(f => path.join(outDir, f));
}

function buildChartSvg(rows) {
  const synth = rows.filter(r => r.synthetic);
  const realRows = rows.filter(r => !r.synthetic);
  const W = 780, H = 430;
  const padL = 78, padR = 32, padT = 50, padB = 64;
  const plotW = W - padL - padR;
  const plotH = H - padT - padB;

  const minX = 5, maxX = 4000;
  const minY = 8, maxY = 80000;
  const xPos = n => padL + ((Math.log10(Math.max(minX, n)) - Math.log10(minX)) / (Math.log10(maxX) - Math.log10(minX))) * plotW;
  const yPos = ms => padT + plotH - ((Math.log10(Math.max(minY, Math.min(maxY, ms))) - Math.log10(minY)) / (Math.log10(maxY) - Math.log10(minY))) * plotH;

  const esc = s => String(s).replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
  const parts = [
    `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${W} ${H}" width="${W}" height="${H}" role="img" aria-label="NMTS layout and render time benchmark chart">`,
    `<rect x="0" y="0" width="${W}" height="${H}" rx="10" fill="#ffffff" stroke="#d0d7de"/>`,
    `<text x="${padL}" y="28" font-family="Inter,system-ui,sans-serif" font-size="15" font-weight="600" fill="#1f2328">NMTS Fragment Layout &amp; Render Time (log&#x2013;log, ms vs. entities)</text>`,
  ];

  // Grid lines
  for (const xv of [5, 10, 30, 100, 300, 1000, 3000]) {
    const x = xPos(xv).toFixed(1);
    parts.push(`<line x1="${x}" y1="${padT}" x2="${x}" y2="${padT + plotH}" stroke="#eef1f5" stroke-width="1"/>`);
    parts.push(`<text x="${x}" y="${padT + plotH + 18}" text-anchor="middle" font-family="Inter,system-ui,sans-serif" font-size="11" fill="#57606a">${xv}</text>`);
  }
  for (const yv of [10, 100, 1000, 10000, 35000]) {
    const y = yPos(yv).toFixed(1);
    const label = yv === 35000 ? '35s timeout' : yv >= 1000 ? `${yv / 1000}s` : `${yv}ms`;
    parts.push(`<line x1="${padL}" y1="${y}" x2="${padL + plotW}" y2="${y}" stroke="${yv === 35000 ? '#f5c2c7' : '#eef1f5'}" stroke-width="${yv === 35000 ? '1.3' : '1'}"${yv === 35000 ? ' stroke-dasharray="5 3"' : ''}/>`);
    parts.push(`<text x="${padL - 8}" y="${(Number(y) + 4).toFixed(1)}" text-anchor="end" font-family="Inter,system-ui,sans-serif" font-size="11" fill="${yv === 35000 ? '#cf222e' : '#57606a'}">${label}</text>`);
  }
  parts.push(`<line x1="${padL}" y1="${padT + plotH}" x2="${padL + plotW}" y2="${padT + plotH}" stroke="#8c959f" stroke-width="1.2"/>`);
  parts.push(`<line x1="${padL}" y1="${padT}" x2="${padL}" y2="${padT + plotH}" stroke="#8c959f" stroke-width="1.2"/>`);
  parts.push(`<text x="${(padL + plotW / 2).toFixed(1)}" y="${H - 12}" text-anchor="middle" font-family="Inter,system-ui,sans-serif" font-size="12" fill="#24292f">NMTS entities in fragment (log scale)</text>`);
  parts.push(`<text x="18" y="${(padT + plotH / 2).toFixed(1)}" text-anchor="middle" transform="rotate(-90 18 ${(padT + plotH / 2).toFixed(1)})" font-family="Inter,system-ui,sans-serif" font-size="12" fill="#24292f">Wall-clock time (ms)</text>`);

  const series = [
    { key: 'nmtscliMs', name: 'nmtscli export dot (Prolog)', color: '#9a6700', dash: '6 3', width: '2.3' },
    { key: 'dotMs', name: 'Graphviz dot -Tsvg', color: '#cf222e', dash: '', width: '2.3' },
    { key: 'sdDefaultMs', name: 'smarter-diagrams (default)', color: '#0969da', dash: '', width: '2.6' },
    { key: 'sdFastMs', name: 'smarter-diagrams (--no-explore-candidates)', color: '#1a7f37', dash: '3 3', width: '2.2' },
  ];

  for (const s of series) {
    const pts = synth.filter(r => typeof r[s.key] === 'number').map(r => `${xPos(r.entities).toFixed(1)},${yPos(r[s.key]).toFixed(1)}`);
    if (pts.length >= 2) {
      parts.push(`<polyline fill="none" stroke="${s.color}" stroke-width="${s.width}"${s.dash ? ` stroke-dasharray="${s.dash}"` : ''} points="${pts.join(' ')}"/>`);
    }
    for (const r of synth) {
      if (typeof r[s.key] === 'number') {
        const cx = xPos(r.entities).toFixed(1);
        const cy = yPos(r[s.key]).toFixed(1);
        parts.push(`<circle cx="${cx}" cy="${cy}" r="${s.key === 'sdFastMs' ? '3.0' : '3.8'}" fill="${s.color}" stroke="#ffffff" stroke-width="1"/>`);
      } else if (r[s.key + 'Failed']) {
        const cx = xPos(r.entities).toFixed(1);
        const cy = yPos(TIMEOUT_MS).toFixed(1);
        parts.push(`<path d="M ${Number(cx) - 4.5} ${Number(cy) - 4.5} L ${Number(cx) + 4.5} ${Number(cy) + 4.5} M ${Number(cx) + 4.5} ${Number(cy) - 4.5} L ${Number(cx) - 4.5} ${Number(cy) + 4.5}" stroke="${s.color}" stroke-width="2.2"/>`);
      }
    }
  }

  // Real NMTS examples square markers
  for (const r of realRows) {
    if (typeof r.dotTotalMs === 'number') {
      const cx = xPos(r.entities).toFixed(1);
      const cy = yPos(r.dotTotalMs).toFixed(1);
      parts.push(`<rect x="${(Number(cx) - 4).toFixed(1)}" y="${(Number(cy) - 4).toFixed(1)}" width="8" height="8" rx="1" fill="#cf222e" stroke="#ffffff" stroke-width="1"/>`);
    }
    if (typeof r.sdDefaultMs === 'number') {
      const cx = xPos(r.entities).toFixed(1);
      const cy = yPos(r.sdDefaultMs).toFixed(1);
      parts.push(`<rect x="${(Number(cx) - 4).toFixed(1)}" y="${(Number(cy) - 4).toFixed(1)}" width="8" height="8" rx="1" fill="#0969da" stroke="#ffffff" stroke-width="1"/>`);
    }
  }

  // Legend positioned safely below 35s timeout line in open upper-left space
  const lx = padL + 14;
  const ly = Math.round(yPos(35000)) + 14;
  parts.push(`<rect x="${lx}" y="${ly}" width="330" height="106" rx="6" fill="#ffffff" fill-opacity="0.95" stroke="#d0d7de"/>`);
  series.forEach((s, idx) => {
    const yy = ly + 17 + idx * 17;
    parts.push(`<line x1="${lx + 10}" y1="${yy}" x2="${lx + 32}" y2="${yy}" stroke="${s.color}" stroke-width="${s.width}"${s.dash ? ` stroke-dasharray="${s.dash}"` : ''}/>`);
    parts.push(`<text x="${lx + 40}" y="${yy + 4}" font-family="Inter,system-ui,sans-serif" font-size="11" fill="#24292f">${esc(s.name)}</text>`);
  });
  const ySq = ly + 88;
  parts.push(`<rect x="${lx + 12}" y="${ySq - 4}" width="8" height="8" rx="1" fill="#cf222e"/><rect x="${lx + 22}" y="${ySq - 4}" width="8" height="8" rx="1" fill="#0969da"/>`);
  parts.push(`<text x="${lx + 40}" y="${ySq + 4}" font-family="Inter,system-ui,sans-serif" font-size="11" fill="#57606a">Squares: real nmts/v1/examples (total vs. sd); &#x2715;: timeout</text>`);

  parts.push('</svg>\n');
  return parts.join('\n');
}

function main() {
  if (process.argv.includes('--chart-only')) {
    const report = JSON.parse(fs.readFileSync(path.join(ROOT, 'docs', 'nmts-benchmark.json'), 'utf8'));
    fs.mkdirSync(path.join(ROOT, 'site', 'public', 'figures'), { recursive: true });
    fs.writeFileSync(path.join(ROOT, 'site', 'public', 'figures', 'nmts-benchmark-chart.svg'), buildChartSvg(report.results));
    console.log('Regenerated site/public/figures/nmts-benchmark-chart.svg from docs/nmts-benchmark.json');
    return;
  }
  fs.mkdirSync(WORK_DIR, { recursive: true });
  fs.mkdirSync(path.join(ROOT, 'docs'), { recursive: true });
  fs.mkdirSync(path.join(ROOT, 'site', 'public', 'figures'), { recursive: true });

  const ethDir = path.join(NMTS_REPO, 'v1', 'examples', 'ethernet');
  const intDir = path.join(NMTS_REPO, 'v1', 'examples', 'internal_fabric');
  const satFiles = ensureSatelliteMefExpanded();

  const cases = [
    {
      id: 'internal_fabric',
      label: 'nmts/v1/examples/internal_fabric',
      synthetic: false,
      files: [path.join(intDir, 'single_switch.txtpb')],
    },
    {
      id: 'ethernet',
      label: 'nmts/v1/examples/ethernet',
      synthetic: false,
      files: ['sdn_agent.txtpb', 'sw1.txtpb', 'sw2.txtpb', 'sw1_sw2_link.txtpb'].map(f => path.join(ethDir, f)),
    },
    {
      id: 'satellite_mef',
      label: 'nmts/v1/examples/satellite_mef',
      synthetic: false,
      files: satFiles,
    },
  ];

  // 8 entities per platform: 24, 64, 160, 320, 640, 1280, 2560
  for (const platforms of [3, 8, 20, 40, 80, 160, 320]) {
    cases.push({
      id: `synth-${platforms * 8}`,
      label: `Synthetic constellation (${platforms} platforms, ${platforms * 8} entities)`,
      synthetic: true,
      files: generateSyntheticFragment(platforms),
    });
  }

  let nmtscliBroken = false;
  let dotBroken = false;
  let sdDefaultBroken = false;
  let sdFastBroken = false;

  const results = [];
  for (const c of cases) {
    const lower = lowerNmtsToDiagram(c.files);
    const diagPath = path.join(WORK_DIR, `${c.id}.diagram`);
    fs.writeFileSync(diagPath, lower.source);

    const row = {
      id: c.id,
      label: c.label,
      synthetic: c.synthetic,
      entities: lower.entityCount,
      edges: lower.edgeCount,
      lowerMs: Number(lower.lowerMs.toFixed(2)),
    };

    // 1. nmtscli export dot + dot -Tsvg
    if (!nmtscliBroken) {
      const cliRes = medianTime(() => runTimed(NMTSCLI, ['export', 'dot', ...c.files]), !c.synthetic || lower.entityCount <= 200);
      if (!cliRes.ok) {
        row.nmtscliMsFailed = cliRes.reason;
        nmtscliBroken = true;
      } else {
        row.nmtscliMs = Number(cliRes.ms.toFixed(2));
        if (!dotBroken) {
          const dotRes = medianTime(() => runTimed(DOT_BIN, ['-Tsvg'], { input: cliRes.stdout }), !c.synthetic || lower.entityCount <= 200);
          if (!dotRes.ok) {
            row.dotMsFailed = dotRes.reason;
            dotBroken = true;
          } else {
            row.dotMs = Number(dotRes.ms.toFixed(2));
            row.dotTotalMs = Number((row.nmtscliMs + row.dotMs).toFixed(2));
            row.dotDims = parseSvgDims(dotRes.stdout);
            if (c.id === 'ethernet') {
              fs.writeFileSync(path.join(ROOT, 'site', 'public', 'figures', 'nmts-mid-graphviz.svg'), dotRes.stdout);
            }
          }
        }
      }
    }

    // 2. smarter-diagrams (default)
    if (!sdDefaultBroken) {
      const sdRes = medianTime(() => runTimed(SD_BIN, [diagPath]), !c.synthetic || lower.entityCount <= 200);
      if (!sdRes.ok) {
        row.sdDefaultMsFailed = sdRes.reason;
        sdDefaultBroken = true;
      } else {
        row.sdDefaultMs = Number(sdRes.ms.toFixed(2));
        row.sdDims = parseSvgDims(sdRes.stdout);
        if (c.id === 'ethernet') {
          fs.writeFileSync(path.join(ROOT, 'site', 'public', 'figures', 'nmts-mid-smarter.svg'), sdRes.stdout);
        }
      }
    }

    // 3. smarter-diagrams --no-explore-candidates
    if (!sdFastBroken) {
      const sdFastRes = medianTime(() => runTimed(SD_BIN, [diagPath, '--no-explore-candidates']), !c.synthetic || lower.entityCount <= 200);
      if (!sdFastRes.ok) {
        row.sdFastMsFailed = sdFastRes.reason;
        sdFastBroken = true;
      } else {
        row.sdFastMs = Number(sdFastRes.ms.toFixed(2));
        if (!row.sdDims) row.sdDims = parseSvgDims(sdFastRes.stdout);
      }
    }

    results.push(row);
    console.log(JSON.stringify(row));
  }

  const report = {
    generatedAt: new Date().toISOString(),
    timeoutMs: TIMEOUT_MS,
    environment: {
      os: process.platform,
      arch: process.arch,
      node: process.version,
      graphvizVersion: spawnSync(DOT_BIN, ['-V'], { encoding: 'utf8' }).stderr.trim(),
    },
    results,
  };

  fs.writeFileSync(path.join(ROOT, 'docs', 'nmts-benchmark.json'), JSON.stringify(report, null, 2) + '\n');
  fs.writeFileSync(path.join(ROOT, 'site', 'public', 'figures', 'nmts-benchmark-chart.svg'), buildChartSvg(results));
  console.log('Wrote docs/nmts-benchmark.json and site/public/figures/nmts-benchmark-chart.svg');
}

main();
