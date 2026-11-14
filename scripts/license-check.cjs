const assert = require('node:assert/strict');
const { execFileSync } = require('node:child_process');
const fs = require('node:fs');
const path = require('node:path');

const root = path.resolve(__dirname, '..');

const FORBIDDEN_PATTERNS = [
  /\bAGPL\b/i,
  /\bGPL\b/i,
  /\bLGPL\b/i,
  /\bMPL\b/i,
  /\bCDDL\b/i,
  /\bEPL\b/i,
  /\bEUPL\b/i,
  /\bCC-BY-SA\b/i,
];

const ALLOWED_TOKENS = new Set([
  'MIT',
  'Apache-2.0',
  'BSD-2-Clause',
  'BSD-3-Clause',
  'BSL-1.0',
  'ISC',
  'Zlib',
  'Unlicense',
  '0BSD',
  'Unicode-3.0',
]);

function resolvedTree(extraArgs) {
  const out = execFileSync(
    'cargo',
    ['tree', '--locked', '--edges', 'normal', '--prefix', 'none', '--format', '{p}', ...extraArgs],
    { cwd: root, encoding: 'utf8', maxBuffer: 16 * 1024 * 1024 }
  );
  const set = new Set();
  for (const line of out.split(/\r?\n/)) {
    const m = line.match(/^([^\s]+)\s+v([^\s]+)/);
    if (m && m[1] !== 'smarter-diagrams') {
      set.add(`${m[1]}@${m[2]}`);
    }
  }
  return set;
}

function validateLicenseExpr(pkgId, expr) {
  assert(expr && expr.trim().length > 0, `${pkgId}: missing SPDX license expression`);
  for (const re of FORBIDDEN_PATTERNS) {
    assert(!re.test(expr), `${pkgId}: forbidden copyleft/weak-copyleft license ${expr}`);
  }
  const tokens = expr
    .replace(/[()]/g, ' ')
    .split(/\s+(?:OR|AND)\s+|\/|\s+/)
    .map(s => s.trim())
    .filter(Boolean);
  for (const tok of tokens) {
    assert(ALLOWED_TOKENS.has(tok), `${pkgId}: unreviewed SPDX token "${tok}" in "${expr}"`);
  }
}

function main() {
  const meta = JSON.parse(
    execFileSync('cargo', ['metadata', '--format-version', '1', '--locked', '--all-features'], {
      cwd: root,
      encoding: 'utf8',
      maxBuffer: 64 * 1024 * 1024,
    })
  );

  const core = resolvedTree(['--no-default-features']);
  const wasm = resolvedTree(['--no-default-features', '--features', 'wasm,wasm-runtime']);
  const exportProfile = resolvedTree(['--no-default-features', '--features', 'export']);
  const verification = resolvedTree(['--no-default-features', '--features', 'verification']);
  const codeBindings = resolvedTree(['--no-default-features', '--features', 'code-bindings']);

  const workspacePkg = meta.packages.find(p => p.source === null);
  assert(workspacePkg && workspacePkg.license === 'MIT', 'Workspace crate must be MIT licensed');

  // Verify feature isolation: Wasm bundles never pull native rasterization/PDF/font crates.
  for (const forbiddenInWasm of ['resvg', 'usvg', 'svg2pdf', 'tiny-skia', 'fontdb', 'pdf-writer']) {
    assert(
      ![...wasm].some(id => id.startsWith(`${forbiddenInWasm}@`)),
      `Wasm feature graph must not depend on ${forbiddenInWasm}`
    );
  }

  const thirdParty = meta.packages
    .filter(p => p.source !== null)
    .sort((a, b) => a.name.localeCompare(b.name) || a.version.localeCompare(b.version));

  const distinctLicenses = new Set();
  const rows = [];
  for (const p of thirdParty) {
    const id = `${p.name}@${p.version}`;
    validateLicenseExpr(id, p.license);
    distinctLicenses.add(p.license);

    const profiles = [
      core.has(id) ? 'core' : null,
      wasm.has(id) ? 'wasm' : null,
      exportProfile.has(id) ? 'export' : null,
      verification.has(id) ? 'verification' : null,
      codeBindings.has(id) ? 'code-bindings' : null,
    ].filter(Boolean);

    rows.push({ id, name: p.name, version: p.version, license: p.license, profiles });
  }

  // Also ensure locked resvg / usvg are not downgraded to historical MPL-2.0 versions.
  const resvg = rows.find(r => r.name === 'resvg');
  const usvg = rows.find(r => r.name === 'usvg');
  const svg2pdf = rows.find(r => r.name === 'svg2pdf');
  assert(resvg && !/MPL/i.test(resvg.license), 'resvg must be permissive (0.45+ Apache-2.0 OR MIT)');
  assert(usvg && !/MPL/i.test(usvg.license), 'usvg must be permissive (0.45+ Apache-2.0 OR MIT)');
  assert(svg2pdf && !/MPL/i.test(svg2pdf.license), 'svg2pdf must remain permissive');

  if (process.argv.includes('--print-table')) {
    console.log('| Crate | Version | SPDX License | Feature Profiles |');
    console.log('| --- | --- | --- | --- |');
    for (const r of rows) {
      const prof = r.profiles.length ? r.profiles.map(p => `\`${p}\``).join(', ') : '`dev / bench`';
      console.log(`| \`${r.name}\` | \`${r.version}\` | \`${r.license}\` | ${prof} |`);
    }
    return;
  }

  const thirdPartyDoc = path.join(root, 'THIRD_PARTY_LICENSES.md');
  if (fs.existsSync(thirdPartyDoc)) {
    const docText = fs.readFileSync(thirdPartyDoc, 'utf8');
    for (const r of rows) {
      assert(
        docText.includes(`\`${r.name}\``),
        `THIRD_PARTY_LICENSES.md is missing dependency \`${r.name}\` (${r.version})`
      );
    }
  }

  console.log(
    `License audit passed: ${rows.length} third-party crates across all features (${wasm.size} in wasm/wasm-runtime, ${exportProfile.size} in export, 0 copyleft/MPL crates; resvg=${resvg.version} [${resvg.license}], usvg=${usvg.version} [${usvg.license}], svg2pdf=${svg2pdf.version} [${svg2pdf.license}]).`
  );
}

main();
