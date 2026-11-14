const assert = require('node:assert/strict');
const { execFileSync, spawnSync } = require('node:child_process');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');

const root = path.resolve(__dirname, '..');

function resolveBinary() {
  if (process.env.SMARTER_DIAGRAMS_BIN && fs.existsSync(process.env.SMARTER_DIAGRAMS_BIN)) {
    return process.env.SMARTER_DIAGRAMS_BIN;
  }
  const exe = process.platform === 'win32' ? '.exe' : '';
  const debug = path.join(root, 'target', 'debug', `smarter-diagrams${exe}`);
  const release = path.join(root, 'target', 'release', `smarter-diagrams${exe}`);
  if (fs.existsSync(debug) && fs.existsSync(release)) {
    return fs.statSync(debug).mtimeMs >= fs.statSync(release).mtimeMs ? debug : release;
  }
  if (fs.existsSync(debug)) return debug;
  if (fs.existsSync(release)) return release;
  execFileSync('cargo', ['build', '--locked', '--features', 'export'], { cwd: root, stdio: 'inherit' });
  return debug;
}

function checkNvimAvailable() {
  const probe = spawnSync('nvim', ['--version'], { encoding: 'utf8' });
  if (probe.error) {
    throw new Error(`nvim executable required for test:neovim (${probe.error.message})`);
  }
  return probe.stdout.split(/\r?\n/)[0] || 'nvim';
}

function runLuaScript(luaPath) {
  const res = spawnSync('nvim', ['--headless', '-u', 'NONE', '-n', '-c', `luafile ${luaPath}`, '-c', 'qa!'], {
    cwd: root,
    encoding: 'utf8',
    timeout: 30000,
  });
  if (res.status !== 0 || (res.stderr && res.stderr.includes('LUA_TEST_ERROR:'))) {
    throw new Error(`nvim exited with ${res.status}:\nSTDOUT:\n${res.stdout}\nSTDERR:\n${res.stderr}`);
  }
  return res;
}

function main() {
  const nvimVersion = checkNvimAvailable();
  const bin = resolveBinary();
  const tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), 'sd-nvim-test-'));

  try {
    const diagramPath = path.join(tmpDir, 'sample.diagram');
    fs.writeFileSync(diagramPath, 'client -> api: HTTPS\napi -> db: SQL\n');

    // 1. Real Neovim fallback test when vim.ui.img is absent (< 0.13) + mocked 0.13 vim.ui.img inline preview + LSP startup
    const resultJson = path.join(tmpDir, 'result.json');
    const luaTest = path.join(tmpDir, 'test.lua');
    const rtp = path.join(root, 'extensions', 'neovim').replace(/\\/g, '/');
    const binLua = bin.replace(/\\/g, '/');
    const diagramLua = diagramPath.replace(/\\/g, '/');
    const resultLua = resultJson.replace(/\\/g, '/');

    fs.writeFileSync(
      luaTest,
      `
local ok, err = xpcall(function()
vim.opt.runtimepath:append('${rtp}')
vim.cmd('edit ' .. vim.fn.fnameescape('${diagramLua}'))

local opened_urls = {}
local sd = require('smarter_diagrams')
sd.setup({
  binary = '${binLua}',
  on_open = function(url) table.insert(opened_urls, url) end,
})

local had_native_ui_img = sd.has_ui_img()
local fallback_res = nil
if not had_native_ui_img then
  fallback_res = sd.preview_inline({ silent = true })
  vim.wait(3000, function() return #opened_urls > 0 end, 25)
  assert(fallback_res.mode == 'browser', 'expected browser fallback when vim.ui.img missing')
  assert(#opened_urls >= 1, 'expected on_open URL during fallback DiagramPreview')
end

-- Mock Neovim 0.13 vim.ui.img provider API
local img_store = {}
local next_id = 0
local del_calls = 0
vim.ui.img = {
  set = function(bytes, opts)
    next_id = next_id + 1
    img_store[next_id] = { len = #bytes, header = string.sub(bytes, 1, 8), opts = opts }
    return next_id
  end,
  get = function(id)
    return img_store[id]
  end,
  del = function(id)
    del_calls = del_calls + 1
    img_store[id] = nil
  end,
}

assert(sd.has_ui_img(), 'has_ui_img must return true when vim.ui.img.set exists')
local inline_done = false
local inline_ok = false
local inline_res = sd.preview_inline({ theme = 'catppuccin-mocha' }, function(ok)
  inline_ok = ok
  inline_done = true
end)
assert(inline_res.mode == 'vim.ui.img', 'expected vim.ui.img mode')
vim.wait(8000, function() return inline_done end, 25)
assert(inline_ok, 'inline preview callback reported failure: ' .. tostring(sd.inline_state().last_error))

local st = sd.inline_state()
assert(st.img_id ~= nil and img_store[st.img_id] ~= nil, 'expected active image handle in mock vim.ui.img')
assert(img_store[st.img_id].header == '\\137PNG\\r\\n\\26\\n', 'expected valid PNG header passed to vim.ui.img.set')
assert(img_store[st.img_id].len > 100, 'expected non-empty PNG bytes')

sd.stop_preview_inline()
assert(del_calls >= 1, 'expected vim.ui.img.del to be called on DiagramPreviewStop')

-- Verify LSP client attached or starts cleanly on diagram buffer
local client_id = vim.lsp.start({
  name = 'smarter-diagrams-test',
  cmd = { '${binLua}', 'lsp' },
  root_dir = vim.fn.getcwd(),
})
assert(client_id and client_id > 0, 'expected LSP client start to succeed')
local lsp_ready = vim.wait(3000, function()
  local c = vim.lsp.get_client_by_id(client_id)
  return c and c.server_capabilities and c.server_capabilities.definitionProvider == true
end, 25)
assert(lsp_ready, 'expected LSP server initialize capabilities with definitionProvider=true')
vim.lsp.stop_client(client_id, true)

local out = vim.json.encode({
  had_native_ui_img = had_native_ui_img,
  fallback_urls = #opened_urls,
  png_bytes = st.last_png_bytes,
  del_calls = del_calls,
  lsp_ready = lsp_ready,
})
vim.fn.writefile({ out }, '${resultLua}')
end, debug.traceback)
if not ok then
  vim.api.nvim_err_writeln('LUA_TEST_ERROR: ' .. tostring(err))
  vim.cmd('cquit 1')
end
`
    );

    runLuaScript(luaTest);
    const report = JSON.parse(fs.readFileSync(resultJson, 'utf8'));
    assert.ok(report.png_bytes > 100, 'PNG payload missing');
    assert.ok(report.del_calls >= 1, 'vim.ui.img.del not invoked');
    assert.equal(report.lsp_ready, true, 'LSP initialize not received');
    console.log(
      `Neovim plugin test passed (${nvimVersion}): native_ui_img=${report.had_native_ui_img}, fallback_urls=${report.fallback_urls}, png_bytes=${report.png_bytes}, del_calls=${report.del_calls}, lsp_ready=${report.lsp_ready}`
    );
  } finally {
    fs.rmSync(tmpDir, { recursive: true, force: true });
  }
}

main();
