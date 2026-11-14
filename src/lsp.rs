//! Language Server Protocol (LSP) over stdio JSON-RPC 2.0.
//!
//! Provides real-time diagnostics (syntax, model, layout, and lint findings),
//! go-to-definition, find-references, hover documentation, safe identifier
//! rename (`prepareRename` + `rename`), and lossless document formatting.

use crate::model::{Diagnostic, Severity, Side, ViewKind};
use crate::syntax::{self, Kind, Parse, Span, SyntaxNode, Token};
use crate::{Layout, LayoutOptions, SceneGraph};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeSet, HashMap};
use std::io::{BufRead, Write};

/// Run the blocking LSP stdio loop until EOF or `exit`.
pub fn serve(mut reader: impl BufRead, mut writer: impl Write) -> std::io::Result<()> {
    let mut server = Server::new();
    while let Some(msg) = read_frame(&mut reader)? {
        let (responses, should_exit) = server.handle_value(msg);
        for out in responses {
            write_frame(&mut writer, &out)?;
        }
        writer.flush()?;
        if should_exit {
            break;
        }
    }
    Ok(())
}

/// Stateful in-process LSP request/notification handler.
#[derive(Default)]
pub struct Server {
    docs: HashMap<String, String>,
    shutdown_requested: bool,
}

impl Server {
    pub fn new() -> Self {
        Self::default()
    }

    /// Process one incoming JSON-RPC message and return outgoing messages plus an `exit` flag.
    pub fn handle_value(&mut self, msg: Value) -> (Vec<Value>, bool) {
        let id = msg.get("id").cloned();
        let Some(method) = msg.get("method").and_then(Value::as_str) else {
            return (Vec::new(), false);
        };
        let params = msg.get("params").cloned().unwrap_or(Value::Null);
        let mut out = Vec::new();
        let mut should_exit = false;

        match method {
            "initialize" => {
                if let Some(id) = id {
                    out.push(ok_response(
                        id,
                        json!({
                            "capabilities": {
                                "textDocumentSync": {
                                    "openClose": true,
                                    "change": 1,
                                    "save": { "includeText": false }
                                },
                                "definitionProvider": true,
                                "referencesProvider": true,
                                "hoverProvider": true,
                                "documentFormattingProvider": true,
                                "renameProvider": {
                                    "prepareProvider": true
                                }
                            },
                            "serverInfo": {
                                "name": "smarter-diagrams-lsp",
                                "version": env!("CARGO_PKG_VERSION")
                            }
                        }),
                    ));
                }
            }
            "initialized" | "$/cancelRequest" | "$/setTrace" => {}
            "shutdown" => {
                self.shutdown_requested = true;
                if let Some(id) = id {
                    out.push(ok_response(id, Value::Null));
                }
            }
            "exit" => {
                should_exit = true;
            }
            "textDocument/didOpen" => {
                if let Some(td) = params.get("textDocument")
                    && let (Some(uri), Some(text)) = (
                        td.get("uri").and_then(Value::as_str),
                        td.get("text").and_then(Value::as_str),
                    )
                {
                    let uri = uri.to_string();
                    let text = text.to_string();
                    let diags = publish_diagnostics_notification(&uri, &text);
                    self.docs.insert(uri, text);
                    out.push(diags);
                }
            }
            "textDocument/didChange" => {
                if let Some(uri) = params
                    .pointer("/textDocument/uri")
                    .and_then(Value::as_str)
                    .map(str::to_string)
                    && let Some(changes) = params.get("contentChanges").and_then(Value::as_array)
                    && let Some(last_text) = changes
                        .last()
                        .and_then(|c| c.get("text"))
                        .and_then(Value::as_str)
                {
                    let text = last_text.to_string();
                    let diags = publish_diagnostics_notification(&uri, &text);
                    self.docs.insert(uri, text);
                    out.push(diags);
                }
            }
            "textDocument/didClose" => {
                if let Some(uri) = params.pointer("/textDocument/uri").and_then(Value::as_str) {
                    self.docs.remove(uri);
                    out.push(json!({
                        "jsonrpc": "2.0",
                        "method": "textDocument/publishDiagnostics",
                        "params": {
                            "uri": uri,
                            "diagnostics": []
                        }
                    }));
                }
            }
            "textDocument/definition" => {
                if let Some(id) = id {
                    out.push(self.handle_definition(id, &params));
                }
            }
            "textDocument/references" => {
                if let Some(id) = id {
                    out.push(self.handle_references(id, &params));
                }
            }
            "textDocument/hover" => {
                if let Some(id) = id {
                    out.push(self.handle_hover(id, &params));
                }
            }
            "textDocument/prepareRename" => {
                if let Some(id) = id {
                    out.push(self.handle_prepare_rename(id, &params));
                }
            }
            "textDocument/rename" => {
                if let Some(id) = id {
                    out.push(self.handle_rename(id, &params));
                }
            }
            "textDocument/formatting" => {
                if let Some(id) = id {
                    out.push(self.handle_formatting(id, &params));
                }
            }
            _ => {
                if let Some(id) = id {
                    out.push(err_response(
                        id,
                        -32601,
                        format!("method not found: {method}"),
                    ));
                }
            }
        }
        (out, should_exit)
    }

    fn doc_and_pos(&self, params: &Value) -> Option<(&str, &str, Position)> {
        let raw_uri = params
            .pointer("/textDocument/uri")
            .and_then(Value::as_str)?;
        let line = params.pointer("/position/line").and_then(Value::as_u64)? as u32;
        let character = params
            .pointer("/position/character")
            .and_then(Value::as_u64)? as u32;
        let (uri, source) = self.docs.get_key_value(raw_uri)?;
        Some((uri.as_str(), source.as_str(), Position { line, character }))
    }

    fn handle_definition(&self, id: Value, params: &Value) -> Value {
        let Some((uri, source, pos)) = self.doc_and_pos(params) else {
            return ok_response(id, Value::Null);
        };
        let index = analyze(source);
        let byte = index.lines.pos_to_byte(source, pos);
        let Some(occ) = index.occurrence_at(byte) else {
            return ok_response(id, Value::Null);
        };
        let defs = index.definitions(&occ.target);
        if defs.is_empty() {
            return ok_response(id, Value::Null);
        }
        if defs.len() == 1 {
            return ok_response(
                id,
                json!({
                    "uri": uri,
                    "range": index.lines.span_to_range(source, defs[0].span)
                }),
            );
        }
        let locs: Vec<Value> = defs
            .into_iter()
            .map(|d| {
                json!({
                    "uri": uri,
                    "range": index.lines.span_to_range(source, d.span)
                })
            })
            .collect();
        ok_response(id, Value::Array(locs))
    }

    fn handle_references(&self, id: Value, params: &Value) -> Value {
        let Some((uri, source, pos)) = self.doc_and_pos(params) else {
            return ok_response(id, json!([]));
        };
        let include_declaration = params
            .pointer("/context/includeDeclaration")
            .and_then(Value::as_bool)
            .unwrap_or(true);
        let index = analyze(source);
        let byte = index.lines.pos_to_byte(source, pos);
        let Some(occ) = index.occurrence_at(byte) else {
            return ok_response(id, json!([]));
        };
        let mut seen = BTreeSet::new();
        let mut locs = Vec::new();
        for r in index.references(&occ.target, include_declaration) {
            if seen.insert((r.span.start, r.span.end)) {
                locs.push(json!({
                    "uri": uri,
                    "range": index.lines.span_to_range(source, r.span)
                }));
            }
        }
        ok_response(id, Value::Array(locs))
    }

    fn handle_hover(&self, id: Value, params: &Value) -> Value {
        let Some((_, source, pos)) = self.doc_and_pos(params) else {
            return ok_response(id, Value::Null);
        };
        let index = analyze(source);
        let byte = index.lines.pos_to_byte(source, pos);
        let Some(occ) = index.occurrence_at(byte) else {
            return ok_response(id, Value::Null);
        };
        let layout = crate::compile_with(source, &LayoutOptions::default(), None)
            .ok()
            .map(|d| d.layout);
        let Some(markdown) = render_hover(&index.scene, layout.as_ref(), &occ.target) else {
            return ok_response(id, Value::Null);
        };
        ok_response(
            id,
            json!({
                "contents": {
                    "kind": "markdown",
                    "value": markdown
                },
                "range": index.lines.span_to_range(source, occ.span)
            }),
        )
    }

    fn handle_prepare_rename(&self, id: Value, params: &Value) -> Value {
        let Some((_, source, pos)) = self.doc_and_pos(params) else {
            return ok_response(id, Value::Null);
        };
        let index = analyze(source);
        let byte = index.lines.pos_to_byte(source, pos);
        let Some(occ) = index.occurrence_at(byte) else {
            return ok_response(id, Value::Null);
        };
        ok_response(
            id,
            json!({
                "range": index.lines.span_to_range(source, occ.span),
                "placeholder": occ.span.text(source)
            }),
        )
    }

    fn handle_rename(&self, id: Value, params: &Value) -> Value {
        let Some((uri, source, pos)) = self.doc_and_pos(params) else {
            return ok_response(id, Value::Null);
        };
        let Some(new_name) = params.get("newName").and_then(Value::as_str) else {
            return err_response(id, -32602, "missing newName parameter");
        };
        if !is_valid_rename_ident(new_name) {
            return err_response(
                id,
                -32602,
                format!("'{new_name}' is not a valid diagram identifier"),
            );
        }
        let index = analyze(source);
        let byte = index.lines.pos_to_byte(source, pos);
        let Some(occ) = index.occurrence_at(byte) else {
            return ok_response(id, Value::Null);
        };
        let mut seen = BTreeSet::new();
        let mut edits = Vec::new();
        for r in index.references(&occ.target, true) {
            if seen.insert((r.span.start, r.span.end)) {
                edits.push(json!({
                    "range": index.lines.span_to_range(source, r.span),
                    "newText": new_name
                }));
            }
        }
        let mut changes = serde_json::Map::new();
        changes.insert(uri.to_string(), Value::Array(edits));
        ok_response(id, json!({ "changes": changes }))
    }

    fn handle_formatting(&self, id: Value, params: &Value) -> Value {
        let Some(uri) = params.pointer("/textDocument/uri").and_then(Value::as_str) else {
            return ok_response(id, json!([]));
        };
        let Some(source) = self.docs.get(uri).map(String::as_str) else {
            return ok_response(id, json!([]));
        };
        let formatted = format_source(source);
        if formatted == source {
            return ok_response(id, json!([]));
        }
        let lines = LineIndex::new(source);
        let full_range = lines.span_to_range(source, Span::new(0, source.len()));
        ok_response(
            id,
            json!([{
                "range": full_range,
                "newText": formatted
            }]),
        )
    }
}

fn ok_response(id: Value, result: Value) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": result
    })
}

fn err_response(id: Value, code: i64, message: impl Into<String>) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": {
            "code": code,
            "message": message.into()
        }
    })
}

/// Read one `Content-Length` framed JSON-RPC message from `reader`.
pub fn read_frame(mut reader: impl BufRead) -> std::io::Result<Option<Value>> {
    let mut content_length: Option<usize> = None;
    loop {
        let mut line = String::new();
        let n = reader.read_line(&mut line)?;
        if n == 0 {
            return Ok(None);
        }
        let trimmed = line.trim_end_matches(['\r', '\n']);
        if trimmed.is_empty() {
            break;
        }
        if let Some((name, val)) = trimmed.split_once(':')
            && name.trim().eq_ignore_ascii_case("content-length")
        {
            content_length = val.trim().parse::<usize>().ok();
        }
    }
    let Some(len) = content_length else {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "missing Content-Length header",
        ));
    };
    let mut buf = vec![0u8; len];
    reader.read_exact(&mut buf)?;
    let val: Value = serde_json::from_slice(&buf).map_err(|e| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("invalid JSON payload: {e}"),
        )
    })?;
    Ok(Some(val))
}

/// Write one `Content-Length` framed JSON-RPC message to `writer`.
pub fn write_frame(mut writer: impl Write, value: &Value) -> std::io::Result<()> {
    let body = serde_json::to_vec(value)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    write!(writer, "Content-Length: {}\r\n\r\n", body.len())?;
    writer.write_all(&body)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Position {
    pub line: u32,
    pub character: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Range {
    pub start: Position,
    pub end: Position,
}

/// UTF-8 byte offset <-> LSP UTF-16 line/character index.
pub struct LineIndex {
    line_starts: Vec<usize>,
}

impl LineIndex {
    pub fn new(source: &str) -> Self {
        let mut line_starts = vec![0usize];
        for (i, b) in source.bytes().enumerate() {
            if b == b'\n' {
                line_starts.push(i + 1);
            }
        }
        Self { line_starts }
    }

    pub fn byte_to_pos(&self, source: &str, offset: usize) -> Position {
        let mut clamped = offset.min(source.len());
        while clamped > 0 && !source.is_char_boundary(clamped) {
            clamped -= 1;
        }
        let line_idx = match self.line_starts.binary_search(&clamped) {
            Ok(i) => i,
            Err(i) => i.saturating_sub(1),
        };
        let start = self.line_starts[line_idx];
        let character = source[start..clamped].encode_utf16().count() as u32;
        Position {
            line: line_idx as u32,
            character,
        }
    }

    pub fn span_to_range(&self, source: &str, span: Span) -> Range {
        let s = span.start.min(span.end);
        let e = span.start.max(span.end);
        Range {
            start: self.byte_to_pos(source, s),
            end: self.byte_to_pos(source, e),
        }
    }

    pub fn pos_to_byte(&self, source: &str, pos: Position) -> usize {
        let line_idx = (pos.line as usize).min(self.line_starts.len().saturating_sub(1));
        let start = self.line_starts[line_idx];
        let raw_end = self
            .line_starts
            .get(line_idx + 1)
            .copied()
            .unwrap_or(source.len());
        let line_slice = source[start..raw_end].trim_end_matches(['\r', '\n']);
        let mut utf16 = 0u32;
        for (byte_idx, ch) in line_slice.char_indices() {
            if utf16 >= pos.character {
                return start + byte_idx;
            }
            utf16 += ch.len_utf16() as u32;
        }
        start + line_slice.len()
    }
}

/// Collect syntax, semantic model, layout, and lint diagnostics for a source buffer.
pub fn collect_diagnostics(source: &str) -> Vec<Value> {
    let lines = LineIndex::new(source);
    let parse = syntax::parse(source);
    let scene = crate::model::lower(&parse);
    let mut out = Vec::new();
    let mut has_error = false;

    for (span, msg) in &parse.errors {
        has_error = true;
        out.push(make_diagnostic(
            &lines,
            source,
            *span,
            Severity::Error,
            "smarter-diagrams.syntax",
            None,
            msg,
        ));
    }

    for diag in &scene.diagnostics {
        if diag.severity == Severity::Error {
            has_error = true;
        }
        out.push(make_diagnostic(
            &lines,
            source,
            diag.span,
            diag.severity,
            "smarter-diagrams.model",
            None,
            &diag.message,
        ));
    }

    if !has_error {
        match crate::compile_with(source, &LayoutOptions::default(), None) {
            Ok(diagram) => {
                for finding in crate::lint::lint(&diagram) {
                    let sev = match finding.severity.as_str() {
                        "error" => Severity::Error,
                        _ => Severity::Warning,
                    };
                    out.push(make_diagnostic(
                        &lines,
                        source,
                        finding.span.unwrap_or_else(|| Span::new(0, 0)),
                        sev,
                        "smarter-diagrams.lint",
                        Some(&finding.code),
                        &finding.message,
                    ));
                }
            }
            Err(crate::Error::Layout(msg)) => {
                let first_line_end = source.find('\n').unwrap_or(source.len());
                out.push(make_diagnostic(
                    &lines,
                    source,
                    Span::new(0, first_line_end),
                    Severity::Error,
                    "smarter-diagrams.layout",
                    None,
                    &msg,
                ));
            }
            Err(crate::Error::Diagnostics(diags)) => {
                for diag in diags {
                    out.push(make_diagnostic(
                        &lines,
                        source,
                        diag.span,
                        diag.severity,
                        "smarter-diagrams.model",
                        None,
                        &diag.message,
                    ));
                }
            }
            Err(crate::Error::Edit(msg)) => {
                out.push(make_diagnostic(
                    &lines,
                    source,
                    Span::new(0, 0),
                    Severity::Error,
                    "smarter-diagrams",
                    None,
                    &msg,
                ));
            }
        }
    }
    out
}

fn make_diagnostic(
    lines: &LineIndex,
    source: &str,
    span: Span,
    severity: Severity,
    src: &str,
    code: Option<&str>,
    message: &str,
) -> Value {
    let mut obj = json!({
        "range": lines.span_to_range(source, span),
        "severity": match severity {
            Severity::Error => 1,
            Severity::Warning => 2,
        },
        "source": src,
        "message": message
    });
    if let Some(code) = code {
        obj["code"] = json!(code);
    }
    obj
}

fn publish_diagnostics_notification(uri: &str, source: &str) -> Value {
    json!({
        "jsonrpc": "2.0",
        "method": "textDocument/publishDiagnostics",
        "params": {
            "uri": uri,
            "diagnostics": collect_diagnostics(source)
        }
    })
}

fn _diag_severity(d: &Diagnostic) -> u8 {
    match d.severity {
        Severity::Error => 1,
        Severity::Warning => 2,
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum SymbolKey {
    Node(String),
    Port(String, String),
    Zone(String),
    Flow(String),
    View(String),
}

#[derive(Debug, Clone)]
struct Occurrence {
    target: SymbolKey,
    span: Span,
    is_def: bool,
}

struct SemanticIndex {
    lines: LineIndex,
    scene: SceneGraph,
    occurrences: Vec<Occurrence>,
}

impl SemanticIndex {
    fn occurrence_at(&self, byte: usize) -> Option<&Occurrence> {
        self.occurrences
            .iter()
            .filter(|o| o.span.start <= byte && byte < o.span.end)
            .min_by_key(|o| o.span.end.saturating_sub(o.span.start))
            .or_else(|| {
                self.occurrences
                    .iter()
                    .filter(|o| o.span.start <= byte && byte <= o.span.end)
                    .min_by_key(|o| o.span.end.saturating_sub(o.span.start))
            })
    }

    fn definitions(&self, key: &SymbolKey) -> Vec<&Occurrence> {
        let defs: Vec<_> = self
            .occurrences
            .iter()
            .filter(|o| &o.target == key && o.is_def)
            .collect();
        if !defs.is_empty() {
            return defs;
        }
        // Implicitly introduced node: fall back to its earliest occurrence.
        self.occurrences
            .iter()
            .find(|o| &o.target == key)
            .into_iter()
            .collect()
    }

    fn references(&self, key: &SymbolKey, include_declaration: bool) -> Vec<&Occurrence> {
        self.occurrences
            .iter()
            .filter(|o| &o.target == key && (include_declaration || !o.is_def))
            .collect()
    }
}

fn analyze(source: &str) -> SemanticIndex {
    let lines = LineIndex::new(source);
    let parse = syntax::parse(source);
    let scene = crate::model::lower(&parse);
    let mut occurrences = Vec::new();
    collect_stmt_occurrences(
        &parse,
        &scene,
        source,
        &parse.syntax(),
        "",
        &mut occurrences,
    );
    // Ensure implicitly created nodes (via edges without a NodeDecl) treat their first occurrence as def.
    let explicit_nodes: BTreeSet<String> = occurrences
        .iter()
        .filter(|o| o.is_def)
        .filter_map(|o| match &o.target {
            SymbolKey::Node(id) => Some(id.clone()),
            _ => None,
        })
        .collect();
    let mut promoted = BTreeSet::new();
    for occ in &mut occurrences {
        if let SymbolKey::Node(id) = &occ.target
            && !explicit_nodes.contains(id)
            && promoted.insert(id.clone())
        {
            occ.is_def = true;
        }
    }
    SemanticIndex {
        lines,
        scene,
        occurrences,
    }
}

fn header_tokens(source: &str, stmt: &SyntaxNode) -> Vec<Token> {
    let mut out = Vec::new();
    let mut stack = vec![stmt.clone()];
    let mut raw = Vec::new();
    while let Some(node) = stack.pop() {
        for elem in node
            .children_with_tokens()
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
        {
            match elem {
                rowan::NodeOrToken::Token(t) => {
                    let kind = t.kind();
                    if matches!(kind, Kind::Whitespace | Kind::Newline | Kind::Comment) {
                        continue;
                    }
                    let range = t.text_range();
                    raw.push(Token {
                        kind,
                        span: Span::new(range.start().into(), range.end().into()),
                    });
                }
                rowan::NodeOrToken::Node(child) => {
                    let k = child.kind();
                    if matches!(
                        k,
                        Kind::Endpoint
                            | Kind::PortAnchor
                            | Kind::AttrList
                            | Kind::Group
                            | Kind::Annotation
                            | Kind::KeyVal
                    ) {
                        stack.push(child);
                    }
                }
            }
        }
    }
    raw.sort_by_key(|t| t.span.start);
    for t in raw {
        let _ = source;
        out.push(t);
    }
    out
}

fn collect_stmt_occurrences(
    parse: &Parse,
    scene: &SceneGraph,
    source: &str,
    node: &SyntaxNode,
    scope: &str,
    out: &mut Vec<Occurrence>,
) {
    let _ = parse;
    for child in node.children() {
        match child.kind() {
            Kind::NodeDecl => {
                let ts = header_tokens(source, &child);
                if let Some(first) = ts.first().filter(|t| t.kind == Kind::Ident) {
                    let name = first.text(source);
                    let id = if scope.is_empty() {
                        name.to_string()
                    } else {
                        format!("{scope}.{name}")
                    };
                    out.push(Occurrence {
                        target: SymbolKey::Node(id.clone()),
                        span: first.span,
                        is_def: true,
                    });
                    collect_annotation_node_refs(scene, source, &ts, &id, out);
                    collect_stmt_occurrences(parse, scene, source, &child, &id, out);
                }
            }
            Kind::PortDecl => {
                let ts = header_tokens(source, &child);
                if ts.len() >= 2 && ts[1].kind == Kind::Ident && !scope.is_empty() {
                    out.push(Occurrence {
                        target: SymbolKey::Port(scope.to_string(), ts[1].text(source).to_string()),
                        span: ts[1].span,
                        is_def: true,
                    });
                }
            }
            Kind::EdgeStmt => {
                let ts = header_tokens(source, &child);
                collect_edge_header_refs(scene, source, &ts, scope, out);
                collect_annotation_node_refs(scene, source, &ts, scope, out);
            }
            Kind::ZoneDecl => {
                let ts = header_tokens(source, &child);
                if ts.len() >= 2 && ts[1].kind == Kind::Ident {
                    out.push(Occurrence {
                        target: SymbolKey::Zone(ts[1].text(source).to_string()),
                        span: ts[1].span,
                        is_def: true,
                    });
                }
                collect_bracket_refs(scene, source, &ts, scope, out);
            }
            Kind::AlignDecl | Kind::DistributeDecl => {
                let ts = header_tokens(source, &child);
                collect_bracket_refs(scene, source, &ts, scope, out);
            }
            Kind::FlowDecl => {
                let ts = header_tokens(source, &child);
                if ts.len() >= 2 && ts[1].kind == Kind::Ident {
                    out.push(Occurrence {
                        target: SymbolKey::Flow(ts[1].text(source).to_string()),
                        span: ts[1].span,
                        is_def: true,
                    });
                }
                for sub in child.children().filter(|c| c.kind() == Kind::EdgeStmt) {
                    let sub_ts = header_tokens(source, &sub);
                    collect_edge_header_refs(scene, source, &sub_ts, scope, out);
                }
            }
            Kind::ViewDecl => {
                let ts = header_tokens(source, &child);
                if ts.len() >= 2 && ts[1].kind == Kind::Ident {
                    out.push(Occurrence {
                        target: SymbolKey::View(ts[1].text(source).to_string()),
                        span: ts[1].span,
                        is_def: true,
                    });
                }
                for sub in child.children() {
                    let sub_ts = header_tokens(source, &sub);
                    if sub_ts.first().is_some_and(|t| t.text(source) == "include")
                        && sub_ts.len() > 1
                    {
                        collect_include_refs(scene, source, &sub_ts[1..], scope, out);
                    }
                }
            }
            Kind::StoryDecl => {
                let ts = header_tokens(source, &child);
                let mut i = 2;
                while i < ts.len() {
                    if ts[i].kind == Kind::Ident {
                        i = emit_qualified_chain(scene, source, &ts, i, scope, false, out);
                    } else {
                        i += 1;
                    }
                }
            }
            Kind::LayoutBlock => {
                for sub in child.children() {
                    let sub_ts = header_tokens(source, &sub);
                    let Some(at_pos) = sub_ts.iter().enumerate().position(|(i, t)| {
                        t.kind == Kind::At
                            && sub_ts.get(i + 1).is_some_and(|n| n.kind == Kind::Ident)
                    }) else {
                        continue;
                    };
                    if at_pos == 0 {
                        continue;
                    }
                    if sub_ts[0].kind == Kind::LParen {
                        collect_edge_header_refs(scene, source, &sub_ts[1..at_pos - 1], scope, out);
                    } else if sub_ts[0].kind == Kind::Ident {
                        emit_qualified_chain(
                            scene,
                            source,
                            &sub_ts[..at_pos],
                            0,
                            scope,
                            false,
                            out,
                        );
                    }
                }
            }
            _ => {}
        }
    }
}

fn resolve_in_scene(scene: &SceneGraph, raw: &str, scope: &str) -> String {
    let mut prefix = scope;
    loop {
        let q = if prefix.is_empty() {
            raw.to_string()
        } else {
            format!("{prefix}.{raw}")
        };
        if scene.node_index.contains_key(&q) {
            return q;
        }
        if let Some((p, _)) = prefix.rsplit_once('.') {
            prefix = p;
        } else if !prefix.is_empty() {
            prefix = "";
        } else {
            break;
        }
    }
    if scope.is_empty() {
        raw.to_string()
    } else {
        format!("{scope}.{raw}")
    }
}

/// Emit one `Occurrence` per segment of a dotted identifier sequence (`a.b.c`), stopping
/// before a trailing `.N`/`.E`/`.S`/`.W` side anchor when `stop_before_side` is true.
fn emit_qualified_chain(
    scene: &SceneGraph,
    source: &str,
    ts: &[Token],
    start: usize,
    scope: &str,
    stop_before_side: bool,
    out: &mut Vec<Occurrence>,
) -> usize {
    if ts.get(start).is_none_or(|t| t.kind != Kind::Ident) {
        return start;
    }
    let mut segs = vec![ts[start].clone()];
    let mut i = start + 1;
    while i + 1 < ts.len() && ts[i].kind == Kind::Dot && ts[i + 1].kind == Kind::Ident {
        segs.push(ts[i + 1].clone());
        i += 2;
    }
    let has_side = stop_before_side
        && segs.len() > 1
        && segs
            .last()
            .is_some_and(|t| Side::parse(t.text(source)).is_some());
    let node_segs = if has_side {
        &segs[..segs.len() - 1]
    } else {
        &segs[..]
    };

    let mut path = String::new();
    for (idx, tok) in node_segs.iter().enumerate() {
        if idx > 0 {
            path.push('.');
        }
        path.push_str(tok.text(source));
        let resolved = resolve_in_scene(scene, &path, scope);
        if scene.node_index.contains_key(&resolved) || idx + 1 == node_segs.len() {
            out.push(Occurrence {
                target: SymbolKey::Node(resolved),
                span: tok.span,
                is_def: false,
            });
        }
    }
    i
}

fn collect_edge_header_refs(
    scene: &SceneGraph,
    source: &str,
    ts: &[Token],
    scope: &str,
    out: &mut Vec<Occurrence>,
) {
    let mut depth = 0i32;
    let mut ops = Vec::new();
    let mut suffix = ts.len();
    for (i, t) in ts.iter().enumerate() {
        match t.kind {
            Kind::LParen => depth += 1,
            Kind::RParen => depth -= 1,
            _ => {}
        }
        if depth == 0 {
            if syntax::is_edge(t.kind) {
                ops.push(i);
            } else if matches!(t.kind, Kind::Colon | Kind::LBracket | Kind::At) {
                suffix = i;
                break;
            }
        }
    }
    let mut begin = 0;
    for &op in ops.iter().chain(std::iter::once(&suffix)) {
        if begin < op {
            emit_endpoint_tokens(scene, source, &ts[begin..op], scope, out);
        }
        begin = op + 1;
    }
}

fn emit_endpoint_tokens(
    scene: &SceneGraph,
    source: &str,
    ts: &[Token],
    scope: &str,
    out: &mut Vec<Occurrence>,
) {
    let mut i = emit_qualified_chain(scene, source, ts, 0, scope, true, out);
    // Compute full resolved target node id for any `#port` suffix
    let node_id = out.last().and_then(|o| match &o.target {
        SymbolKey::Node(id) => Some(id.clone()),
        _ => None,
    });
    while i < ts.len() {
        if ts[i].kind == Kind::Hash
            && let Some(port_tok) = ts.get(i + 1).filter(|t| t.kind == Kind::Ident)
            && let Some(ref node_id) = node_id
        {
            out.push(Occurrence {
                target: SymbolKey::Port(node_id.clone(), port_tok.text(source).to_string()),
                span: port_tok.span,
                is_def: false,
            });
            i += 2;
        } else {
            i += 1;
        }
    }
}

fn collect_bracket_refs(
    scene: &SceneGraph,
    source: &str,
    ts: &[Token],
    scope: &str,
    out: &mut Vec<Occurrence>,
) {
    let Some(start) = ts.iter().position(|t| t.kind == Kind::LBracket) else {
        return;
    };
    let mut depth = 0usize;
    let mut end = start;
    for (j, t) in ts.iter().enumerate().skip(start) {
        if t.kind == Kind::LBracket {
            depth += 1;
        } else if t.kind == Kind::RBracket {
            depth -= 1;
            if depth == 0 {
                end = j;
                break;
            }
        }
    }
    let mut at = start + 1;
    while at < end {
        if ts[at].kind == Kind::Comma {
            at += 1;
            continue;
        }
        if ts[at].kind == Kind::Ident {
            at = emit_qualified_chain(scene, source, ts, at, scope, false, out);
        } else {
            at += 1;
        }
    }
}

fn collect_include_refs(
    scene: &SceneGraph,
    source: &str,
    ts: &[Token],
    scope: &str,
    out: &mut Vec<Occurrence>,
) {
    let mut i = 0;
    while i < ts.len() {
        if ts[i].kind == Kind::Ident {
            let next = emit_qualified_chain(scene, source, ts, i, scope, true, out);
            i = next.max(i + 1);
        } else {
            i += 1;
        }
    }
}

fn collect_annotation_node_refs(
    scene: &SceneGraph,
    source: &str,
    ts: &[Token],
    scope: &str,
    out: &mut Vec<Occurrence>,
) {
    // Look for `ref.E` / `ref.N` inside `@pin(...)` or similar annotations
    let mut in_ann = false;
    let mut depth = 0usize;
    let mut i = 0;
    while i < ts.len() {
        if !in_ann
            && ts[i].kind == Kind::At
            && ts.get(i + 1).is_some_and(|t| t.kind == Kind::Ident)
            && ts.get(i + 2).is_some_and(|t| t.kind == Kind::LParen)
        {
            in_ann = true;
            depth = 1;
            i += 3;
            continue;
        }
        if in_ann {
            if ts[i].kind == Kind::LParen {
                depth += 1;
            } else if ts[i].kind == Kind::RParen {
                depth -= 1;
                if depth == 0 {
                    in_ann = false;
                }
                i += 1;
                continue;
            }
            if ts[i].kind == Kind::Ident {
                let mut end = i + 1;
                let mut has_side = false;
                let mut tmp = i;
                while tmp + 2 < ts.len()
                    && ts[tmp + 1].kind == Kind::Dot
                    && ts[tmp + 2].kind == Kind::Ident
                {
                    if Side::parse(ts[tmp + 2].text(source)).is_some() {
                        has_side = true;
                    }
                    tmp += 2;
                    end = tmp + 1;
                }
                if has_side {
                    emit_qualified_chain(scene, source, ts, i, scope, true, out);
                    i = end;
                    continue;
                }
            }
        }
        i += 1;
    }
}

fn is_valid_rename_ident(name: &str) -> bool {
    if name.is_empty() {
        return false;
    }
    let toks = syntax::lex(name);
    toks.len() == 1
        && toks[0].kind == Kind::Ident
        && Side::parse(name).is_none()
        && !matches!(
            name,
            "zone"
                | "align"
                | "distribute"
                | "flow"
                | "view"
                | "step"
                | "scenario"
                | "include"
                | "port"
                | "story"
                | "strata"
                | "theme"
                | "layout"
        )
}

fn render_hover(scene: &SceneGraph, layout: Option<&Layout>, key: &SymbolKey) -> Option<String> {
    let mut md = String::new();
    match key {
        SymbolKey::Node(id) => {
            let node = scene.node(id)?;
            let kind_tag = if node.container { " (container)" } else { "" };
            md.push_str(&format!("**node** `{}`{kind_tag}\n", node.id));
            if !node.label.is_empty() && node.label != node.id {
                let preview = node.label.lines().next().unwrap_or(&node.label);
                md.push_str(&format!("\n- **label**: `{preview}`\n"));
            }
            md.push_str(&format!("- **shape**: `{}`\n", node.shape));
            if let Some(parent) = &node.parent {
                md.push_str(&format!("- **parent**: `{parent}`\n"));
            }
            if let Some(link) = node.link() {
                md.push_str(&format!("- **link**: `{link}`\n"));
            }
            if !node.ports.is_empty() {
                let port_names: Vec<_> = node.ports.keys().cloned().collect();
                md.push_str(&format!("- **ports**: `{}`\n", port_names.join("`, `")));
            }
            let incoming: Vec<_> = scene
                .edges
                .iter()
                .filter(|e| e.target.node == node.id)
                .map(|e| e.source.node.as_str())
                .collect();
            let outgoing: Vec<_> = scene
                .edges
                .iter()
                .filter(|e| e.source.node == node.id)
                .map(|e| e.target.node.as_str())
                .collect();
            if !incoming.is_empty() {
                md.push_str(&format!("- **incoming**: `{}`\n", incoming.join("`, `")));
            }
            if !outgoing.is_empty() {
                md.push_str(&format!("- **outgoing**: `{}`\n", outgoing.join("`, `")));
            }
            if let Some(rect) = layout.and_then(|l| l.nodes.get(&node.id)) {
                md.push_str(&format!(
                    "- **bounds**: center `({:.1}, {:.1})`, size `{:.0}×{:.0}` px\n",
                    rect.x, rect.y, rect.w, rect.h
                ));
            }
        }
        SymbolKey::Port(node_id, port_id) => {
            let node = scene.node(node_id)?;
            let port = node.ports.get(port_id)?;
            md.push_str(&format!("**port** `{node_id}#{port_id}`\n\n"));
            if port.label != port.id {
                md.push_str(&format!("- **label**: `{}`\n", port.label));
            }
            md.push_str(&format!("- **constraint**: `{:?}`\n", port.constraint));
            let connected: Vec<_> = scene
                .edges
                .iter()
                .filter(|e| {
                    (e.source.node == *node_id
                        && e.source.port.as_deref() == Some(port_id.as_str()))
                        || (e.target.node == *node_id
                            && e.target.port.as_deref() == Some(port_id.as_str()))
                })
                .map(|e| e.id.as_str())
                .collect();
            if !connected.is_empty() {
                md.push_str(&format!("- **edges**: `{}`\n", connected.join("`, `")));
            }
        }
        SymbolKey::Zone(id) => {
            let zone = scene.zones.iter().find(|z| z.id == *id)?;
            md.push_str(&format!("**zone** `{}`\n\n", zone.id));
            if zone.label != zone.id {
                md.push_str(&format!("- **label**: `{}`\n", zone.label));
            }
            if !zone.members.is_empty() {
                md.push_str(&format!("- **members**: `{}`\n", zone.members.join("`, `")));
            }
        }
        SymbolKey::Flow(id) => {
            let flow = scene.flows.iter().find(|f| f.id == *id)?;
            md.push_str(&format!("**flow** `{}`\n\n", flow.id));
            if !flow.steps.is_empty() {
                md.push_str(&format!("- **path**: `{}`\n", flow.steps.join(" -> ")));
            }
        }
        SymbolKey::View(id) => {
            let view = scene.views.iter().find(|v| v.id == *id)?;
            let kw = match view.kind {
                ViewKind::View => "view",
                ViewKind::Step => "step",
                ViewKind::Scenario => "scenario",
            };
            md.push_str(&format!("**{kw}** `{}`\n\n", view.id));
            if let Some(label) = &view.label {
                md.push_str(&format!("- **label**: `{label}`\n"));
            }
            if !view.include.is_empty() {
                md.push_str(&format!("- **include**: `{}`\n", view.include.join("`, `")));
            }
        }
    }
    Some(md)
}

/// Lossless source formatter: normalizes 2-space brace indentation, strips trailing
/// horizontal whitespace, collapses consecutive blank lines to at most one, and preserves
/// comments, attributes, and multi-line strings (including embedded code blocks) verbatim.
pub fn format_source(source: &str) -> String {
    let tokens = syntax::lex(source);
    let mut lines: Vec<Vec<(Kind, &str)>> = vec![Vec::new()];
    for tok in tokens {
        if tok.kind == Kind::Newline {
            lines.push(Vec::new());
        } else {
            lines.last_mut().unwrap().push((tok.kind, tok.text(source)));
        }
    }

    let mut depth = 0usize;
    let mut formatted_lines: Vec<String> = Vec::new();
    let mut prev_blank = false;

    for raw_line in lines {
        let trimmed_start = raw_line
            .iter()
            .position(|(k, _)| *k != Kind::Whitespace)
            .unwrap_or(raw_line.len());
        let trimmed_end = raw_line
            .iter()
            .rposition(|(k, _)| *k != Kind::Whitespace)
            .map_or(trimmed_start, |i| i + 1);
        let content = &raw_line[trimmed_start..trimmed_end];

        if content.is_empty() {
            if !formatted_lines.is_empty() && !prev_blank {
                formatted_lines.push(String::new());
                prev_blank = true;
            }
            continue;
        }
        prev_blank = false;

        let leading_closes = content
            .iter()
            .take_while(|(k, _)| *k == Kind::RBrace)
            .count();
        let line_indent = depth.saturating_sub(leading_closes);

        let mut delta: isize = 0;
        for (k, _) in content {
            match k {
                Kind::LBrace => delta += 1,
                Kind::RBrace => delta -= 1,
                _ => {}
            }
        }
        depth = (depth as isize + delta).max(0) as usize;

        let mut line_out = String::new();
        for _ in 0..line_indent {
            line_out.push_str("  ");
        }
        for (_, text) in content {
            line_out.push_str(text);
        }
        formatted_lines.push(line_out);
    }

    while formatted_lines.last().is_some_and(String::is_empty) {
        formatted_lines.pop();
    }
    if formatted_lines.is_empty() {
        return String::new();
    }
    let mut out = formatted_lines.join("\n");
    out.push('\n');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_index_handles_utf16_and_multibyte_positions() {
        let src = "a -> b: \"😀 ok\"\nc -> d\n";
        let lines = LineIndex::new(src);
        let emoji_start = src.find('😀').unwrap();
        let pos = lines.byte_to_pos(src, emoji_start);
        assert_eq!(
            pos,
            Position {
                line: 0,
                character: 9
            }
        );
        let after_emoji = lines.byte_to_pos(src, emoji_start + '😀'.len_utf8());
        assert_eq!(
            after_emoji,
            Position {
                line: 0,
                character: 11
            }
        );
        assert_eq!(lines.pos_to_byte(src, after_emoji), emoji_start + 4);
    }

    #[test]
    fn format_preserves_multiline_code_blocks_and_is_idempotent() {
        let raw = "vpc {\n    api [label: \"\"\"\n```sql\nSELECT 1;\n```\n\"\"\"]\n\n\nclient -> api\n}\n";
        let once = format_source(raw);
        assert_eq!(
            once,
            "vpc {\n  api [label: \"\"\"\n```sql\nSELECT 1;\n```\n\"\"\"]\n\n  client -> api\n}\n"
        );
        assert_eq!(format_source(&once), once);
        assert!(crate::compile(&once).is_ok());
    }

    #[test]
    fn definition_references_hover_rename_work_end_to_end() {
        let mut srv = Server::new();
        let uri = "file:///test.diagram";
        let src = "vpc {\n  api [shape: service, link: \"https://example.com\"]\n}\nclient -> vpc.api: HTTPS\nview overview {\n  include vpc.api\n}\ndistribute row [client, vpc] [gap: 6g]\n";
        let (msgs, _) = srv.handle_value(json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didOpen",
            "params": { "textDocument": { "uri": uri, "languageId": "diagram", "version": 1, "text": src } }
        }));
        assert_eq!(msgs[0]["method"], "textDocument/publishDiagnostics");
        assert_eq!(msgs[0]["params"]["diagnostics"], json!([]));

        // Cursor on `api` in `vpc.api` on line 3 (`client -> vpc.api: HTTPS`)
        let (def_msgs, _) = srv.handle_value(json!({
            "jsonrpc": "2.0", "id": 2, "method": "textDocument/definition",
            "params": { "textDocument": { "uri": uri }, "position": { "line": 3, "character": 15 } }
        }));
        assert_eq!(def_msgs[0]["result"]["range"]["start"]["line"], 1);
        assert_eq!(def_msgs[0]["result"]["range"]["start"]["character"], 2);

        let (ref_msgs, _) = srv.handle_value(json!({
            "jsonrpc": "2.0", "id": 3, "method": "textDocument/references",
            "params": { "textDocument": { "uri": uri }, "position": { "line": 1, "character": 3 }, "context": { "includeDeclaration": true } }
        }));
        assert_eq!(ref_msgs[0]["result"].as_array().unwrap().len(), 3);

        let (hover_msgs, _) = srv.handle_value(json!({
            "jsonrpc": "2.0", "id": 4, "method": "textDocument/hover",
            "params": { "textDocument": { "uri": uri }, "position": { "line": 3, "character": 15 } }
        }));
        let hover_md = hover_msgs[0]["result"]["contents"]["value"]
            .as_str()
            .unwrap();
        assert!(hover_md.contains("vpc.api"));
        assert!(hover_md.contains("https://example.com"));

        let (ren_msgs, _) = srv.handle_value(json!({
            "jsonrpc": "2.0", "id": 5, "method": "textDocument/rename",
            "params": { "textDocument": { "uri": uri }, "position": { "line": 1, "character": 3 }, "newName": "gateway" }
        }));
        assert_eq!(
            ren_msgs[0]["result"]["changes"][uri]
                .as_array()
                .unwrap()
                .len(),
            3
        );
    }
}
