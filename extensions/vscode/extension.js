const vscode = require('vscode');
const { spawn } = require('node:child_process');
function run(source, args) {
  return new Promise((resolve, reject) => {
    const binary = vscode.workspace.getConfiguration('smarterDiagrams').get('binary', 'smarter-diagrams');
    const child = spawn(binary, args, { shell: false });
    let output = '', error = '';
    child.stdout.on('data', data => { output += data; if (output.length > 16000000) child.kill(); });
    child.stderr.on('data', data => { error += data; });
    child.on('error', reject);
    child.on('close', code => code === 0 ? resolve(output) : reject(new Error(error || `Compiler exited ${code}`)));
    child.stdin.on('error', () => {});
    child.stdin.end(source);
  });
}
function activate(context) {
  const diagnostics = vscode.languages.createDiagnosticCollection('smarter-diagrams');
  context.subscriptions.push(diagnostics);
  let panel, current, generation = 0, timer;
  async function refresh(document) {
    if (!panel || document.languageId !== 'smarter-diagrams') return;
    current = document;
    const ticket = ++generation;
    try {
      const svg = await run(document.getText(), ['-', '--format', 'svg']);
      if (ticket !== generation || !panel) return;
      diagnostics.delete(document.uri);
      panel.webview.html = `<!doctype html><meta http-equiv="Content-Security-Policy" content="default-src 'none'; style-src 'unsafe-inline'; img-src data:"><style>body{margin:24px;background:#fafbfd}svg{max-width:100%;height:auto}</style>${svg}`;
    } catch (error) {
      if (ticket !== generation) return;
      diagnostics.set(document.uri, [new vscode.Diagnostic(new vscode.Range(0, 0, 0, 1), error.message, vscode.DiagnosticSeverity.Error)]);
    }
  }
  context.subscriptions.push(vscode.commands.registerCommand('smarterDiagrams.preview', () => {
    const editor = vscode.window.activeTextEditor;
    if (!editor) return;
    if (!panel) {
      panel = vscode.window.createWebviewPanel('smarter-diagrams', 'Diagram preview', vscode.ViewColumn.Beside, { enableScripts: false });
      panel.onDidDispose(() => { panel = undefined; generation++; });
    }
    refresh(editor.document);
  }));
  context.subscriptions.push(vscode.workspace.onDidChangeTextDocument(event => {
    if (!current || event.document.uri.toString() !== current.uri.toString()) return;
    clearTimeout(timer); timer = setTimeout(() => refresh(event.document), 150);
  }));
  context.subscriptions.push(vscode.commands.registerCommand('smarterDiagrams.edit', async () => {
    const editor = vscode.window.activeTextEditor; if (!editor) return;
    const operations = await vscode.window.showInputBox({ title: 'Structured edit JSON array', prompt: 'DiagramEditOp JSON; source changes form one undo step' });
    if (!operations) return;
    try {
      JSON.parse(operations);
      const document = editor.document, version = document.version;
      const text = await run(document.getText(), ['edit', '--operations', operations]);
      if (document.version !== version) throw new Error('Document changed during edit; retry with the current version');
      const edit = new vscode.WorkspaceEdit();
      edit.replace(document.uri, new vscode.Range(document.positionAt(0), document.positionAt(document.getText().length)), text);
      await vscode.workspace.applyEdit(edit);
    } catch (error) { vscode.window.showErrorMessage(error.message); }
  }));
  context.subscriptions.push({ dispose() { clearTimeout(timer); if (panel) panel.dispose(); } });
}
module.exports = { activate };
