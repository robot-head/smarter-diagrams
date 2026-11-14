use serde_json::{Value, json};
use std::io::{BufReader, BufWriter, Write};
use std::process::{Command, Stdio};

fn write_msg(w: &mut impl Write, val: &Value) {
    let body = serde_json::to_vec(val).unwrap();
    write!(w, "Content-Length: {}\r\n\r\n", body.len()).unwrap();
    w.write_all(&body).unwrap();
    w.flush().unwrap();
}

fn read_msg(r: &mut impl std::io::BufRead) -> Value {
    let mut len = None;
    loop {
        let mut line = String::new();
        let n = r.read_line(&mut line).unwrap();
        assert!(n > 0, "unexpected EOF waiting for LSP header");
        let trimmed = line.trim_end_matches(['\r', '\n']);
        if trimmed.is_empty() {
            break;
        }
        if let Some((k, v)) = trimmed.split_once(':')
            && k.trim().eq_ignore_ascii_case("content-length")
        {
            len = Some(v.trim().parse::<usize>().unwrap());
        }
    }
    let mut buf = vec![0u8; len.expect("missing Content-Length")];
    r.read_exact(&mut buf).unwrap();
    serde_json::from_slice(&buf).unwrap()
}

#[test]
fn lsp_subprocess_roundtrip_all_capabilities() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_smarter-diagrams"))
        .arg("lsp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("spawn smarter-diagrams lsp");

    let mut stdin = BufWriter::new(child.stdin.take().unwrap());
    let mut stdout = BufReader::new(child.stdout.take().unwrap());

    write_msg(
        &mut stdin,
        &json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": { "processId": null, "rootUri": null, "capabilities": {} }
        }),
    );
    let init = read_msg(&mut stdout);
    assert_eq!(init["id"], 1);
    assert_eq!(init["result"]["capabilities"]["definitionProvider"], true);
    assert_eq!(init["result"]["capabilities"]["referencesProvider"], true);
    assert_eq!(init["result"]["capabilities"]["hoverProvider"], true);
    assert_eq!(
        init["result"]["capabilities"]["documentFormattingProvider"],
        true
    );
    assert_eq!(
        init["result"]["capabilities"]["renameProvider"]["prepareProvider"],
        true
    );

    write_msg(
        &mut stdin,
        &json!({ "jsonrpc": "2.0", "method": "initialized", "params": {} }),
    );

    let uri = "file:///example.diagram";
    let unformatted = concat!(
        "edge {\n",
        "    api [shape: rect, label: \"Gateway\"] {\n",
        "  port in [side: W]\n",
        "    }\n",
        "}\n",
        "client -> edge.api#in: HTTPS\n",
        "zone prod [edge.api]\n",
        "step s1 {\n",
        "  include edge.api\n",
        "}\n",
        "distribute row [client, edge] [gap: 6g]\n"
    );

    write_msg(
        &mut stdin,
        &json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didOpen",
            "params": {
                "textDocument": {
                    "uri": uri,
                    "languageId": "diagram",
                    "version": 1,
                    "text": unformatted
                }
            }
        }),
    );
    let open_diag = read_msg(&mut stdout);
    assert_eq!(open_diag["method"], "textDocument/publishDiagnostics");
    assert_eq!(open_diag["params"]["diagnostics"], json!([]));

    // Formatting fixes indentation inside `edge { api { ... } }`
    write_msg(
        &mut stdin,
        &json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "textDocument/formatting",
            "params": { "textDocument": { "uri": uri }, "options": { "tabSize": 2, "insertSpaces": true } }
        }),
    );
    let fmt_resp = read_msg(&mut stdout);
    let formatted_text = fmt_resp["result"][0]["newText"].as_str().unwrap();
    assert!(formatted_text.contains("  api [shape: rect, label: \"Gateway\"] {"));
    assert!(formatted_text.contains("    port in [side: W]"));

    // Go-to-definition from `edge.api` on line 5 (`client -> edge.api#in: HTTPS`)
    write_msg(
        &mut stdin,
        &json!({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "textDocument/definition",
            "params": {
                "textDocument": { "uri": uri },
                "position": { "line": 5, "character": 16 }
            }
        }),
    );
    let def_resp = read_msg(&mut stdout);
    assert_eq!(def_resp["result"]["range"]["start"]["line"], 1);
    assert_eq!(def_resp["result"]["range"]["start"]["character"], 4);

    // Find references for `api` (decl + edge + zone + step)
    write_msg(
        &mut stdin,
        &json!({
            "jsonrpc": "2.0",
            "id": 4,
            "method": "textDocument/references",
            "params": {
                "textDocument": { "uri": uri },
                "position": { "line": 1, "character": 5 },
                "context": { "includeDeclaration": true }
            }
        }),
    );
    let refs_resp = read_msg(&mut stdout);
    assert_eq!(refs_resp["result"].as_array().unwrap().len(), 4);

    // Hover shows bounds, port list, and incoming edge info
    write_msg(
        &mut stdin,
        &json!({
            "jsonrpc": "2.0",
            "id": 5,
            "method": "textDocument/hover",
            "params": {
                "textDocument": { "uri": uri },
                "position": { "line": 5, "character": 16 }
            }
        }),
    );
    let hover_resp = read_msg(&mut stdout);
    let md = hover_resp["result"]["contents"]["value"].as_str().unwrap();
    assert!(md.contains("edge.api"));
    assert!(md.contains("bounds"));

    // PrepareRename & Rename on `in` port inside `edge.api#in`
    write_msg(
        &mut stdin,
        &json!({
            "jsonrpc": "2.0",
            "id": 6,
            "method": "textDocument/prepareRename",
            "params": {
                "textDocument": { "uri": uri },
                "position": { "line": 5, "character": 20 }
            }
        }),
    );
    let prep_resp = read_msg(&mut stdout);
    assert_eq!(prep_resp["result"]["placeholder"], "in");

    write_msg(
        &mut stdin,
        &json!({
            "jsonrpc": "2.0",
            "id": 7,
            "method": "textDocument/rename",
            "params": {
                "textDocument": { "uri": uri },
                "position": { "line": 5, "character": 20 },
                "newName": "ingress"
            }
        }),
    );
    let rename_resp = read_msg(&mut stdout);
    assert_eq!(
        rename_resp["result"]["changes"][uri]
            .as_array()
            .unwrap()
            .len(),
        2
    );

    // Broken document emits syntax/model diagnostics
    write_msg(
        &mut stdin,
        &json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didChange",
            "params": {
                "textDocument": { "uri": uri, "version": 2 },
                "contentChanges": [{ "text": "a ->\nzone bad [missing]\n" }]
            }
        }),
    );
    let err_diag = read_msg(&mut stdout);
    assert!(
        !err_diag["params"]["diagnostics"]
            .as_array()
            .unwrap()
            .is_empty()
    );

    // Shutdown + exit cleanly
    write_msg(
        &mut stdin,
        &json!({ "jsonrpc": "2.0", "id": 8, "method": "shutdown", "params": null }),
    );
    let shut_resp = read_msg(&mut stdout);
    assert_eq!(shut_resp["id"], 8);
    assert_eq!(shut_resp["result"], Value::Null);

    write_msg(
        &mut stdin,
        &json!({ "jsonrpc": "2.0", "method": "exit", "params": null }),
    );
    drop(stdin);
    let status = child.wait().unwrap();
    assert!(status.success());
}
