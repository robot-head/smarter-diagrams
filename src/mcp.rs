//! MCP JSON-RPC stdio tools for rendering, diagnostics and version-checked source edits.
use crate::{
    Document,
    edit::{DiagramEditOp, Placement},
};
use serde_json::{Value, json};
use std::io::{BufRead, Write};
#[derive(Default)]
pub struct Server {
    document: Option<Document>,
}
impl Server {
    pub fn handle(&mut self, request: Value) -> Option<Value> {
        let id = request.get("id")?.clone();
        let method = request["method"].as_str().unwrap_or("");
        let result: Result<Value, String> = match method {
            "initialize" => Ok(
                json!({"protocolVersion":"2024-11-05","capabilities":{"tools":{"listChanged":false}},"serverInfo":{"name":"smarter-diagrams","version":env!("CARGO_PKG_VERSION")}}),
            ),
            "ping" => Ok(json!({})),
            "tools/list" => Ok(json!({"tools":tools()})),
            "tools/call" => {
                let p = &request["params"];
                self.call(p["name"].as_str().unwrap_or(""),&p["arguments"]).map(|v|json!({"content":[{"type":"text","text":serde_json::to_string(&v).unwrap()}],"isError":false}))
            }
            _ => Err(format!("unknown method {method}")),
        };
        Some(match result {
            Ok(result) => json!({"jsonrpc":"2.0","id":id,"result":result}),
            Err(message) => {
                json!({"jsonrpc":"2.0","id":id,"error":{"code":-32602,"message":message}})
            }
        })
    }
    fn call(&mut self, name: &str, args: &Value) -> Result<Value, String> {
        match name {
            "diagram_open" => {
                let source = args["source"].as_str().ok_or("source must be a string")?;
                let doc = Document::new(source).map_err(|e| e.to_string())?;
                let result = json!({"revision":doc.revision(),"svg":doc.snapshot().to_svg()});
                self.document = Some(doc);
                Ok(result)
            }
            "diagram_render" => {
                let doc = self.document.as_ref().ok_or("open a diagram first")?;
                Ok(
                    json!({"revision":doc.revision(),"svg":doc.snapshot().to_svg(),"layout":doc.snapshot().layout}),
                )
            }
            "diagram_edit" => {
                let revision = args["revision"]
                    .as_u64()
                    .ok_or("revision must be an unsigned integer")?;
                let ops: Vec<DiagramEditOp> = serde_json::from_value(args["operations"].clone())
                    .map_err(|e| e.to_string())?;
                let doc = self.document.as_mut().ok_or("open a diagram first")?;
                doc.edit(revision, &ops, Placement::Auto)
                    .map_err(|e| e.to_string())?;
                Ok(
                    json!({"revision":doc.revision(),"source":doc.source(),"svg":doc.snapshot().to_svg()}),
                )
            }
            "diagram_lint" => {
                let doc = self.document.as_ref().ok_or("open a diagram first")?;
                Ok(json!({"findings":crate::lint::lint(&doc.snapshot())}))
            }
            _ => Err(format!("unknown tool {name}")),
        }
    }
}
fn tools() -> Vec<Value> {
    vec![
        json!({"name":"diagram_open","description":"Open and validate diagram text","inputSchema":{"type":"object","properties":{"source":{"type":"string"}},"required":["source"],"additionalProperties":false}}),
        json!({"name":"diagram_render","description":"Read current SVG and geometry","inputSchema":{"type":"object","properties":{},"additionalProperties":false}}),
        json!({"name":"diagram_edit","description":"Apply a typed, atomic source-preserving edit transaction","inputSchema":{"type":"object","properties":{"revision":{"type":"integer","minimum":0},"operations":{"type":"array","items":{"oneOf":edit_schemas()}}},"required":["revision","operations"],"additionalProperties":false}}),
        json!({"name":"diagram_lint","description":"Inspect aesthetic findings","inputSchema":{"type":"object","properties":{},"additionalProperties":false}}),
    ]
}
fn edit_schemas() -> Vec<Value> {
    let string = json!({"type":"string"});
    let axis = json!({"enum":["Row","Col"]});
    let node_list = json!({"type":"array","items":{"type":"string"},"minItems":2});
    let pin = json!({"oneOf":[{"type":"object","properties":{"AbsoluteGrid":{"type":"object","properties":{"x":{"type":"integer"},"y":{"type":"integer"}},"required":["x","y"],"additionalProperties":false}},"required":["AbsoluteGrid"],"additionalProperties":false},{"type":"object","properties":{"Relative":{"type":"object","properties":{"x":{"type":"string"},"y":{"type":"string"}},"required":["x","y"],"additionalProperties":false}},"required":["Relative"],"additionalProperties":false}]});
    let pin_axis = json!({"type":"object","properties":{"AxisLock":{"type":"object","properties":{"axis":axis,"coord_grid":{"type":"integer"}},"required":["axis","coord_grid"],"additionalProperties":false}},"required":["AxisLock"],"additionalProperties":false});
    let mut pin = pin;
    pin["oneOf"].as_array_mut().unwrap().push(pin_axis);
    let mut constraints = vec![json!({"const":"Free"})];
    for (name, key, value) in [
        ("FixedSide", None, json!(null)),
        (
            "FixedOrder",
            Some("order"),
            json!({"type":"integer","minimum":0}),
        ),
        (
            "FixedRatio",
            Some("ratio"),
            json!({"type":"number","exclusiveMinimum":0,"exclusiveMaximum":1}),
        ),
        (
            "FixedPos",
            Some("offset_grid"),
            json!({"type":"number","minimum":0}),
        ),
    ] {
        let mut properties = json!({"side":{"enum":["N","E","S","W"]}});
        let mut required = vec!["side"];
        if let Some(key) = key {
            properties[key] = value;
            required.push(key);
        }
        constraints.push(json!({"type":"object","properties":{name:{"type":"object","properties":properties,"required":required,"additionalProperties":false}},"required":[name],"additionalProperties":false}));
    }
    let variants = vec![
        (
            "LockPort",
            json!({"node":{"type":"string"},"port":{"type":"string"},"spec":{"oneOf":constraints}}),
            vec!["node", "port", "spec"],
        ),
        (
            "BreakAlignment",
            json!({"node":{"type":"string"},"axis":axis}),
            vec!["node", "axis"],
        ),
        (
            "PinNode",
            json!({"node":string,"pin":pin,"size_lock":{"type":["array","null"],"items":{"type":"integer","minimum":1},"minItems":2,"maxItems":2}}),
            vec!["node", "pin"],
        ),
        (
            "SnapAlign",
            json!({"axis":axis,"members":node_list}),
            vec!["axis", "members"],
        ),
        (
            "UnpinEntities",
            json!({"targets":{"type":"array","items":{"type":"string"}}}),
            vec!["targets"],
        ),
        (
            "SlideLabel",
            json!({"edge":{"type":"string"},"slot":{"enum":["Above","Below","Inline"]},"t_arc":{"type":"number","minimum":0,"maximum":1}}),
            vec!["edge", "slot", "t_arc"],
        ),
        (
            "NudgeRouteChannel",
            json!({"edge":{"type":"string"},"segment_idx":{"type":"integer","minimum":0},"lane_grid":{"type":"number"}}),
            vec!["edge", "segment_idx", "lane_grid"],
        ),
    ];
    variants.into_iter().map(|(name,properties,required)|json!({"type":"object","properties":{name:{"type":"object","properties":properties,"required":required,"additionalProperties":false}},"required":[name],"additionalProperties":false})).collect()
}
pub fn serve(reader: impl BufRead, mut writer: impl Write) -> std::io::Result<()> {
    let mut server = Server::default();
    for line in reader.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let response = match serde_json::from_str::<Value>(&line) {
            Ok(request) => server.handle(request),
            Err(e) => Some(
                json!({"jsonrpc":"2.0","id":null,"error":{"code":-32700,"message":e.to_string()}}),
            ),
        };
        if let Some(response) = response {
            serde_json::to_writer(&mut writer, &response)?;
            writeln!(writer)?;
            writer.flush()?;
        }
    }
    Ok(())
}
