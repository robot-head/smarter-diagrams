local M = {}
local script_root = vim.fn.fnamemodify(debug.getinfo(1, 'S').source:sub(2), ':h:h:h:h:h')
function M.setup(options)
  options = options or {}
  local binary = options.binary or 'smarter-diagrams'
  local source_buf, timer, server = nil, nil, nil
  M.namespace = vim.api.nvim_create_namespace('smarter-diagrams')
  local function update()
    if not source_buf or not vim.api.nvim_buf_is_valid(source_buf) or not server then return end
    local source = table.concat(vim.api.nvim_buf_get_lines(source_buf, 0, -1, false), '\n') .. '\n'
    vim.fn.chansend(server, vim.json.encode({ source = source }) .. '\n')
  end
  local function diagnostic(message)
    if not source_buf or not vim.api.nvim_buf_is_valid(source_buf) then return end
    if message == '' then vim.diagnostic.reset(M.namespace, source_buf)
    else vim.diagnostic.set(M.namespace, source_buf, {{ lnum = 0, col = 0, message = message, severity = vim.diagnostic.severity.ERROR }}) end
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
                if message.error then diagnostic(message.error) end
              end
            end
          end
        end,
        on_stderr = function(_, data) for _, line in ipairs(data) do if line ~= '' then diagnostic(line) end end end,
        on_exit = function() server = nil end,
      })
      if server <= 0 then server = nil; error('Cannot start diagram preview server') end
    end
    update()
  end, {})
  local group = vim.api.nvim_create_augroup('smarter-diagrams', { clear = true })
  vim.api.nvim_create_autocmd({ 'TextChanged', 'TextChangedI' }, { group = group, pattern = '*.diagram', callback = function()
    if timer then timer:stop(); timer:close() end
    timer = vim.uv.new_timer(); timer:start(150, 0, vim.schedule_wrap(update))
  end })
  vim.api.nvim_create_autocmd('VimLeavePre', { group = group, callback = function()
    if server then vim.fn.chanclose(server, 'stdin') end
    if timer then timer:stop(); timer:close() end
  end })
end
return M
