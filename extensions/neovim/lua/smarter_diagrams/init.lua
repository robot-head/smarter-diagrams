local M = {}

local script_root = vim.fn.fnamemodify(debug.getinfo(1, 'S').source:sub(2), ':h:h:h:h:h')
local inline_state = {
  src_buf = nil,
  preview_buf = nil,
  preview_win = nil,
  img_id = nil,
  timer = nil,
  active = false,
  opts = {},
  last_png_bytes = 0,
  last_error = nil,
  mode = nil,
}

function M.has_ui_img()
  return type(vim.ui) == 'table'
    and type(vim.ui.img) == 'table'
    and type(vim.ui.img.set) == 'function'
end

local function register_filetype()
  if vim.filetype and vim.filetype.add then
    vim.filetype.add({
      extension = {
        diagram = 'diagram',
      },
    })
  end
end

local function close_inline_timer()
  if inline_state.timer then
    pcall(function()
      inline_state.timer:stop()
      inline_state.timer:close()
    end)
    inline_state.timer = nil
  end
end

local function clear_inline_img()
  if inline_state.img_id ~= nil and M.has_ui_img() and type(vim.ui.img.del) == 'function' then
    pcall(vim.ui.img.del, inline_state.img_id)
  end
  inline_state.img_id = nil
end

function M.stop_preview_inline()
  close_inline_timer()
  clear_inline_img()
  inline_state.active = false
  if inline_state.preview_win and vim.api.nvim_win_is_valid(inline_state.preview_win) then
    pcall(vim.api.nvim_win_close, inline_state.preview_win, true)
  end
  if inline_state.preview_buf and vim.api.nvim_buf_is_valid(inline_state.preview_buf) then
    pcall(vim.api.nvim_buf_delete, inline_state.preview_buf, { force = true })
  end
  inline_state.preview_win = nil
  inline_state.preview_buf = nil
  inline_state.src_buf = nil
end

function M.inline_state()
  return vim.deepcopy({
    active = inline_state.active,
    mode = inline_state.mode,
    img_id = inline_state.img_id,
    src_buf = inline_state.src_buf,
    preview_buf = inline_state.preview_buf,
    preview_win = inline_state.preview_win,
    last_png_bytes = inline_state.last_png_bytes,
    last_error = inline_state.last_error,
  })
end

function M.setup_lsp(options)
  options = options or {}
  if options.lsp == false then
    return
  end
  register_filetype()
  local binary = options.binary or 'smarter-diagrams'
  local group = vim.api.nvim_create_augroup('smarter-diagrams-lsp', { clear = true })
  local function start_for_buf(buf)
    if not buf or not vim.api.nvim_buf_is_valid(buf) then
      return nil
    end
    local name = vim.api.nvim_buf_get_name(buf)
    local root = options.root_dir
      or (name ~= '' and vim.fs and vim.fs.dirname(name))
      or vim.fn.getcwd()
    return vim.lsp.start({
      name = 'smarter-diagrams',
      cmd = { binary, 'lsp' },
      root_dir = root,
    }, { bufnr = buf })
  end
  vim.api.nvim_create_autocmd('FileType', {
    group = group,
    pattern = 'diagram',
    callback = function(ev)
      start_for_buf(ev.buf)
    end,
  })
  return start_for_buf
end

function M.setup(options)
  options = options or {}
  register_filetype()
  local binary = options.binary or 'smarter-diagrams'
  local source_buf, timer, server = nil, nil, nil
  M.namespace = M.namespace or vim.api.nvim_create_namespace('smarter-diagrams')

  local function set_diagnostic(buf, message)
    if not buf or not vim.api.nvim_buf_is_valid(buf) then
      return
    end
    if not message or message == '' then
      vim.diagnostic.reset(M.namespace, buf)
    else
      vim.diagnostic.set(M.namespace, buf, {
        {
          lnum = 0,
          col = 0,
          message = message,
          severity = vim.diagnostic.severity.ERROR,
        },
      })
    end
  end

  local function update_browser()
    if not source_buf or not vim.api.nvim_buf_is_valid(source_buf) or not server then
      return
    end
    local source = table.concat(vim.api.nvim_buf_get_lines(source_buf, 0, -1, false), '\n') .. '\n'
    vim.fn.chansend(server, vim.json.encode({ source = source }) .. '\n')
  end

  local function ensure_preview_window(origin_win)
    if inline_state.preview_buf and vim.api.nvim_buf_is_valid(inline_state.preview_buf) then
      if inline_state.preview_win and vim.api.nvim_win_is_valid(inline_state.preview_win) then
        return
      end
    end
    vim.cmd('botright vsplit')
    local win = vim.api.nvim_get_current_win()
    local buf = vim.api.nvim_create_buf(false, true)
    vim.api.nvim_win_set_buf(win, buf)
    vim.bo[buf].buftype = 'nofile'
    vim.bo[buf].bufhidden = 'wipe'
    vim.bo[buf].swapfile = false
    vim.bo[buf].filetype = 'smarter-diagrams-preview'
    vim.bo[buf].modifiable = true
    vim.api.nvim_buf_set_lines(buf, 0, -1, false, { 'Smarter Diagrams Inline Preview (vim.ui.img)' })
    vim.bo[buf].modifiable = false
    inline_state.preview_buf = buf
    inline_state.preview_win = win
    vim.api.nvim_create_autocmd('BufWipeout', {
      buffer = buf,
      once = true,
      callback = function()
        clear_inline_img()
        inline_state.active = false
        inline_state.preview_buf = nil
        inline_state.preview_win = nil
      end,
    })
    if origin_win and vim.api.nvim_win_is_valid(origin_win) then
      vim.api.nvim_set_current_win(origin_win)
    end
  end

  local function render_inline_sync_or_async(on_done)
    local buf = inline_state.src_buf
    if not inline_state.active or not buf or not vim.api.nvim_buf_is_valid(buf) then
      if on_done then on_done(false) end
      return
    end
    local source = table.concat(vim.api.nvim_buf_get_lines(buf, 0, -1, false), '\n') .. '\n'
    local cmd = { binary, '-', '--format', 'png' }
    local ropts = inline_state.opts or {}
    if ropts.theme or options.theme then
      table.insert(cmd, '--theme')
      table.insert(cmd, ropts.theme or options.theme)
    end
    if ropts.view or options.view then
      table.insert(cmd, '--view')
      table.insert(cmd, ropts.view or options.view)
    end
    if ropts.step or options.step then
      table.insert(cmd, '--step')
      table.insert(cmd, ropts.step or options.step)
    end

    local function handle_result(code, stdout_bytes, stderr_text)
      if not inline_state.active or not vim.api.nvim_buf_is_valid(buf) then
        if on_done then on_done(false) end
        return
      end
      if code ~= 0 or not stdout_bytes or #stdout_bytes == 0 then
        local msg = (stderr_text and stderr_text ~= '') and stderr_text or ('PNG export failed (exit ' .. tostring(code) .. ')')
        inline_state.last_error = msg
        set_diagnostic(buf, msg)
        if on_done then on_done(false) end
        return
      end
      inline_state.last_error = nil
      inline_state.last_png_bytes = #stdout_bytes
      set_diagnostic(buf, '')
      if inline_state.preview_win and vim.api.nvim_win_is_valid(inline_state.preview_win) then
        clear_inline_img()
        local win = inline_state.preview_win
        vim.api.nvim_win_call(win, function()
          local width = math.max(10, vim.api.nvim_win_get_width(win) - 2)
          local height = math.max(5, vim.api.nvim_win_get_height(win) - 2)
          inline_state.img_id = vim.ui.img.set(stdout_bytes, {
            row = 1,
            col = 0,
            width = width,
            height = height,
            zindex = 50,
          })
        end)
      end
      if on_done then on_done(true) end
    end

    if vim.system then
      vim.system(cmd, { stdin = source, text = false }, vim.schedule_wrap(function(res)
        handle_result(res.code, res.stdout, res.stderr)
      end))
    else
      local stdout_bytes = vim.fn.system(cmd, source)
      handle_result(vim.v.shell_error, stdout_bytes, vim.v.shell_error == 0 and '' or tostring(stdout_bytes))
    end
  end

  function M.preview_inline(opts, on_done)
    opts = opts or {}
    if not M.has_ui_img() then
      inline_state.active = false
      inline_state.mode = 'browser'
      if not opts.silent then
        vim.notify('smarter-diagrams: vim.ui.img requires Neovim 0.13+; falling back to browser preview', vim.log.levels.INFO)
      end
      vim.cmd('DiagramPreview')
      if on_done then on_done(true) end
      return { mode = 'browser' }
    end
    local origin_win = vim.api.nvim_get_current_win()
    inline_state.src_buf = vim.api.nvim_get_current_buf()
    inline_state.opts = opts
    inline_state.active = true
    inline_state.mode = 'vim.ui.img'
    ensure_preview_window(origin_win)
    render_inline_sync_or_async(on_done)
    return { mode = 'vim.ui.img' }
  end

  vim.api.nvim_create_user_command('DiagramPreview', function()
    source_buf = vim.api.nvim_get_current_buf()
    if not server then
      server = vim.fn.jobstart({ options.node or 'node', options.server or (script_root .. '/scripts/preview-server.cjs'), binary }, {
        on_stdout = function(_, data)
          for _, line in ipairs(data) do
            if line ~= '' then
              local ok, message = pcall(vim.json.decode, line)
              if ok then
                if message.url then
                  if options.on_open then options.on_open(message.url) else vim.ui.open(message.url) end
                end
                if message.error then set_diagnostic(source_buf, message.error) end
              end
            end
          end
        end,
        on_stderr = function(_, data)
          for _, line in ipairs(data) do
            if line ~= '' then set_diagnostic(source_buf, line) end
          end
        end,
        on_exit = function() server = nil end,
      })
      if server <= 0 then server = nil; error('Cannot start diagram preview server') end
    end
    update_browser()
  end, {})

  vim.api.nvim_create_user_command('DiagramPreviewInline', function(cmd_opts)
    local opts = {}
    if cmd_opts and cmd_opts.args and cmd_opts.args ~= '' then
      opts.theme = cmd_opts.args
    end
    M.preview_inline(opts)
  end, { nargs = '?' })

  vim.api.nvim_create_user_command('DiagramPreviewStop', function()
    M.stop_preview_inline()
  end, {})

  local group = vim.api.nvim_create_augroup('smarter-diagrams', { clear = true })
  vim.api.nvim_create_autocmd({ 'TextChanged', 'TextChangedI', 'BufWritePost' }, {
    group = group,
    pattern = '*.diagram',
    callback = function()
      if server then
        if timer then timer:stop(); timer:close() end
        timer = vim.uv.new_timer()
        timer:start(150, 0, vim.schedule_wrap(update_browser))
      end
      if inline_state.active and inline_state.src_buf == vim.api.nvim_get_current_buf() then
        close_inline_timer()
        inline_state.timer = vim.uv.new_timer()
        inline_state.timer:start(150, 0, vim.schedule_wrap(function()
          render_inline_sync_or_async(nil)
        end))
      end
    end,
  })
  vim.api.nvim_create_autocmd('VimLeavePre', {
    group = group,
    callback = function()
      M.stop_preview_inline()
      if server then vim.fn.chanclose(server, 'stdin') end
      if timer then pcall(function() timer:stop(); timer:close() end) end
    end,
  })

  M.setup_lsp(options)
end

return M
