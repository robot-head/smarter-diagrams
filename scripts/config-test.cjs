const assert = require('node:assert/strict');
const { execFileSync, spawnSync } = require('node:child_process');
const fs = require('node:fs');
const path = require('node:path');

const root = path.resolve(__dirname, '..');

function readText(relPath) {
  return fs.readFileSync(path.join(root, relPath), 'utf8');
}

function parseYamlIfAvailable(relPath) {
  const full = path.join(root, relPath);
  const res = spawnSync(
    'python3',
    [
      '-c',
      'import json, sys, yaml; print(json.dumps(yaml.safe_load(open(sys.argv[1]))))',
      full,
    ],
    { encoding: 'utf8' }
  );
  if (res.status === 0) {
    return JSON.parse(res.stdout);
  }
  return null;
}

function testRenovateConfig() {
  const raw = readText('renovate.json');
  const cfg = JSON.parse(raw);

  assert.equal(cfg.$schema, 'https://docs.renovatebot.com/renovate-schema.json');
  assert.equal(cfg.automerge, true);
  assert.equal(cfg.automergeType, 'pr');
  assert.equal(cfg.platformAutomerge, true);
  assert.equal(cfg.ignoreTests, false);
  assert.equal(cfg.osvVulnerabilityAlerts, true);
  assert.equal(cfg.vulnerabilityAlerts?.enabled, true);
  assert.equal(cfg.vulnerabilityAlerts?.automerge, true);
  assert.equal(cfg.lockFileMaintenance?.enabled, true);
  assert.equal(cfg.lockFileMaintenance?.automerge, true);

  for (const preset of [
    'config:best-practices',
    ':enablePreCommit',
    'customManagers:dockerfileVersions',
    'customManagers:githubActionsVersions',
    'customManagers:makefileVersions',
  ]) {
    assert(cfg.extends.includes(preset), `renovate.json missing preset ${preset}`);
  }

  assert.equal(cfg['git-submodules']?.enabled, true);
  assert.equal(cfg.nix?.enabled, true);
  assert(Array.isArray(cfg.postUpdateOptions) && cfg.postUpdateOptions.includes('gomodTidy'));

  const gomodRule = (cfg.packageRules || []).find(
    r => Array.isArray(r.matchManagers) && r.matchManagers.includes('gomod')
  );
  assert(gomodRule && gomodRule.enabled === true, 'expected gomod indirect rule');

  const wasmRule = (cfg.packageRules || []).find(
    r =>
      Array.isArray(r.matchPackageNames) &&
      r.matchPackageNames.includes('wasm-bindgen') &&
      r.matchPackageNames.includes('wasm-bindgen/wasm-bindgen')
  );
  assert(wasmRule && wasmRule.groupName, 'expected wasm-bindgen group rule');

  assert(Array.isArray(cfg.customManagers) && cfg.customManagers.length > 0);
  const wasmMgr = cfg.customManagers.find(m => m.depNameTemplate === 'wasm-bindgen/wasm-bindgen');
  assert(wasmMgr, 'expected wasm-bindgen customManager');

  for (const pat of wasmMgr.managerFilePatterns || []) {
    const inner = pat.startsWith('/') && pat.endsWith('/') ? pat.slice(1, -1) : pat;
    const re = new RegExp(inner);
    assert(
      re.test('.github/workflows/pages-build.yml'),
      `managerFilePatterns ${pat} must match .github/workflows/pages-build.yml`
    );
  }

  const pagesBuildText = readText('.github/workflows/pages-build.yml');
  for (const ms of wasmMgr.matchStrings || []) {
    // Reject lookarounds and numeric backreferences unsupported by RE2.
    assert(!/\(\?(?:[=!]|<[=!])|\\[1-9]/.test(ms), `customManager regex must be RE2-compatible: ${ms}`);
    const re = new RegExp(ms);
    const match = pagesBuildText.match(re);
    assert(match && match.groups && /^\d+\.\d+\.\d+$/.test(match.groups.currentValue), 'expected semver currentValue match in pages-build.yml');
  }
}

function testWorkflowsAndGate() {
  const ciText = readText('.github/workflows/ci.yml');
  const pagesBuildText = readText('.github/workflows/pages-build.yml');
  const pagesText = readText('.github/workflows/pages.yml');

  assert(!ciText.includes('pages: write'), 'ci.yml must not request pages: write');
  assert(!ciText.includes('id-token: write'), 'ci.yml must not request id-token: write');
  assert(!pagesBuildText.includes('pages: write'), 'pages-build.yml must not request pages: write');
  assert(!pagesBuildText.includes('id-token: write'), 'pages-build.yml must not request id-token: write');
  assert(pagesText.includes('pages-build.yml'), 'pages.yml should reuse pages-build.yml');

  const ci = parseYamlIfAvailable('.github/workflows/ci.yml');
  if (ci) {
    assert.equal(ci.permissions?.contents, 'read');
    const jobs = ci.jobs || {};
    const jobIds = Object.keys(jobs);
    assert(jobIds.includes('gate'), 'ci.yml must define a gate job');
    const gate = jobs.gate;
    assert.equal(gate.name, 'gate', 'gate job display name must match ruleset context "gate"');
    assert.equal(gate.if, 'always()', 'gate job must run with if: always()');
    const siblingJobs = jobIds.filter(id => id !== 'gate').sort();
    const gateNeeds = [...(gate.needs || [])].sort();
    assert.deepEqual(gateNeeds, siblingJobs, 'gate job must depend on all sibling CI jobs');

    const checkStep = (gate.steps || []).find(s => typeof s.run === 'string' && s.run.includes('NEEDS_JSON'));
    assert(checkStep, 'gate job must inspect NEEDS_JSON');

    // Execute the exact bash step from ci.yml against dynamic CI scenarios.
    const scenarios = [
      {
        name: 'all_pass',
        needs: { rust: { result: 'success' }, checks: { result: 'success' }, pages: { result: 'success' } },
        expectCode: 0,
      },
      {
        name: 'dynamic_skip_passes',
        needs: { rust: { result: 'skipped' }, checks: { result: 'success' }, pages: { result: 'skipped' } },
        expectCode: 0,
      },
      {
        name: 'failure_blocks_gate',
        needs: { rust: { result: 'failure' }, checks: { result: 'success' }, pages: { result: 'skipped' } },
        expectCode: 1,
      },
      {
        name: 'cancelled_blocks_gate',
        needs: { rust: { result: 'success' }, checks: { result: 'cancelled' }, pages: { result: 'success' } },
        expectCode: 1,
      },
      {
        name: 'empty_needs_blocks_gate',
        needs: {},
        expectCode: 1,
      },
    ];

    for (const sc of scenarios) {
      const res = spawnSync('bash', ['-e', '-c', checkStep.run], {
        env: { ...process.env, NEEDS_JSON: JSON.stringify(sc.needs) },
        encoding: 'utf8',
      });
      assert.equal(
        res.status,
        sc.expectCode,
        `gate scenario ${sc.name} returned ${res.status} (expected ${sc.expectCode}): ${res.stdout} ${res.stderr}`
      );
    }
  } else {
    assert(ciText.includes('gate:'), 'ci.yml must define gate job');
    assert(ciText.includes('if: always()'), 'gate job must use if: always()');
    assert(ciText.includes('NEEDS_JSON'), 'gate job must evaluate NEEDS_JSON');
  }
}

function testExtensionManifests() {
  const vscodePkg = JSON.parse(readText('extensions/vscode/package.json'));
  assert.equal(vscodePkg.name, 'smarter-diagrams');
  assert(fs.existsSync(path.join(root, 'extensions/vscode', vscodePkg.main)));
  execFileSync(process.execPath, ['--check', path.join(root, 'extensions/vscode/extension.js')]);

  const goMod = readText('extensions/hugo/go.mod');
  assert(goMod.includes('module github.com/robot-head/smarter-diagrams/extensions/hugo'));
  assert(/^go\s+\d+\.\d+/m.test(goMod));

  const compose = readText('extensions/kroki/docker-compose.yml');
  assert(compose.includes('yuzutech/kroki:'));
  assert(fs.existsSync(path.join(root, 'extensions/kroki/Dockerfile')));
  assert(fs.existsSync(path.join(root, 'extensions/neovim/lua/smarter_diagrams/init.lua')));
}

function main() {
  testRenovateConfig();
  testWorkflowsAndGate();
  testExtensionManifests();
  console.log('Config, workflow gate, Renovate, and extension checks passed.');
}

main();
