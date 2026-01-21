use std::io::{self, BufRead, BufReader, Write};

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::assets::{list_icon_ids, list_sound_ids};
use crate::notify::{notify, NotifyInput};

const PROTOCOL_VERSION: &str = "2024-11-05";
const SERVER_NAME: &str = "toastmcp";
const SERVER_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Deserialize)]
struct RpcRequest {
    #[allow(dead_code)]
    jsonrpc: String,
    id: Option<Value>,
    method: String,
    #[serde(default)]
    params: Value,
}

#[derive(Debug, Serialize)]
struct RpcResponse {
    jsonrpc: &'static str,
    id: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<RpcError>,
}

#[derive(Debug, Serialize)]
struct RpcError {
    code: i64,
    message: String,
}

#[derive(Debug, Serialize)]
struct ToolDescription {
    name: &'static str,
    description: &'static str,
    #[serde(rename = "inputSchema")]
    input_schema: Value,
}

#[derive(Debug, Serialize)]
struct ResourceDescription {
    uri: &'static str,
    name: &'static str,
    description: &'static str,
    #[serde(rename = "mimeType")]
    mime_type: &'static str,
}

pub fn run() -> Result<()> {
    let stdin = io::stdin();
    let mut reader = BufReader::new(stdin.lock());
    let stdout = io::stdout();
    let mut writer = io::BufWriter::new(stdout.lock());

    loop {
        let message = match read_message(&mut reader)? {
            Some(message) => message,
            None => break,
        };
        let request: RpcRequest = serde_json::from_str(&message.payload)
            .with_context(|| format!("Invalid JSON-RPC payload: {}", message.payload))?;
        if let Some(response) = handle_request(request)? {
            write_message(&mut writer, &response, message.framing)?;
        }
    }

    Ok(())
}

fn handle_request(request: RpcRequest) -> Result<Option<RpcResponse>> {
    match request.method.as_str() {
        "initialize" => Ok(Some(handle_initialize(request))),
        "tools/list" => Ok(Some(handle_tools_list(request))),
        "tools/call" => Ok(Some(handle_tools_call(request))),
        "resources/list" => Ok(Some(handle_resources_list(request))),
        "resources/read" => Ok(Some(handle_resources_read(request))),
        "resource-templates/list" => Ok(Some(handle_resource_templates_list(request))),
        "ping" => Ok(Some(ok_response(request, Value::Null))),
        _ => {
            if let Some(id) = request.id {
                Ok(Some(error_response(
                    id,
                    -32601,
                    format!("Method not found: {}", request.method),
                )))
            } else {
                Ok(None)
            }
        }
    }
}

fn handle_initialize(request: RpcRequest) -> RpcResponse {
    let protocol_version = request
        .params
        .get("protocolVersion")
        .and_then(Value::as_str)
        .unwrap_or(PROTOCOL_VERSION)
        .to_string();

    ok_response(
        request,
        serde_json::json!({
            "protocolVersion": protocol_version,
            "capabilities": {
                "tools": {},
                "resources": {}
            },
            "serverInfo": {
                "name": SERVER_NAME,
                "version": SERVER_VERSION
            }
        }),
    )
}

fn handle_tools_list(request: RpcRequest) -> RpcResponse {
    let icon_ids = list_icon_ids();
    let sound_ids = list_sound_ids();
    let icon_schema = if icon_ids.is_empty() {
        serde_json::json!({
            "type": "string",
            "description": "Required. Icon id from icons/ folder (without extension). Do not guess; add icons or call tools/list for the current enum."
        })
    } else {
        serde_json::json!({
            "type": "string",
            "enum": icon_ids,
            "description": "Required. Must be one of the enum values (no guessing)."
        })
    };
    let sound_schema = if sound_ids.is_empty() {
        serde_json::json!({
            "type": "string",
            "description": "Required. Sound id from sounds/ folder (without extension). Do not guess; add sounds or call tools/list for the current enum."
        })
    } else {
        serde_json::json!({
            "type": "string",
            "enum": sound_ids,
            "description": "Required. Must be one of the enum values (no guessing)."
        })
    };

    let tool = ToolDescription {
        name: "notify",
        description: "Send a system toast + sound. Use only the provided icon/sound ids (no guessing); call tools/list to see the current enums.",
        input_schema: serde_json::json!({
            "type": "object",
            "additionalProperties": false,
            "properties": {
                "title": {
                    "type": "string",
                    "description": "Very short description of the current project (5 words or less)."
                },
                "message": { "type": "string" },
                "sound": sound_schema,
                "icon": icon_schema
            },
            "required": ["title", "message", "sound", "icon"]
        }),
    };

    ok_response(
        request,
        serde_json::json!({
            "tools": [
                tool,
                {
                    "name": "list_assets",
                    "description": "List available icon and sound ids for ToastMCP.",
                    "inputSchema": {
                        "type": "object",
                        "additionalProperties": false,
                        "properties": {}
                    }
                }
            ]
        }),
    )
}

fn handle_tools_call(request: RpcRequest) -> RpcResponse {
    let Some(id) = request.id else {
        return error_response(
            Value::Null,
            -32600,
            "Missing id for tools/call".to_string(),
        );
    };

    let name = request
        .params
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or("");

    if name == "list_assets" {
        let icons = list_icon_ids();
        let sounds = list_sound_ids();
        let result = serde_json::json!({
            "content": [
                {"type": "text", "text": serde_json::json!({"icons": icons, "sounds": sounds}).to_string()}
            ]
        });
        return RpcResponse {
            jsonrpc: "2.0",
            id,
            result: Some(result),
            error: None,
        };
    }

    if name != "notify" {
        return error_response(id, -32602, format!("Unknown tool: {name}"));
    }

    let args_value = request
        .params
        .get("arguments")
        .cloned()
        .unwrap_or(Value::Null);

    let args: NotifyInput = match serde_json::from_value(args_value) {
        Ok(args) => args,
        Err(err) => {
            return error_response(id, -32602, format!("Invalid arguments: {err}"));
        }
    };

    let result = match notify(args) {
        Ok(()) => serde_json::json!({
            "content": [
                {"type": "text", "text": "Notification sent."}
            ]
        }),
        Err(err) => serde_json::json!({
            "content": [
                {"type": "text", "text": format!("Notification failed: {err}") }
            ],
            "isError": true
        }),
    };

    RpcResponse {
        jsonrpc: "2.0",
        id,
        result: Some(result),
        error: None,
    }
}

fn handle_resources_list(request: RpcRequest) -> RpcResponse {
    let resources = vec![ResourceDescription {
        uri: "toastmcp://assets",
        name: "ToastMCP assets",
        description: "Lists available icon and sound ids.",
        mime_type: "application/json",
    }];

    ok_response(
        request,
        serde_json::json!({
            "resources": resources
        }),
    )
}

fn handle_resource_templates_list(request: RpcRequest) -> RpcResponse {
    ok_response(
        request,
        serde_json::json!({
            "resourceTemplates": []
        }),
    )
}

fn handle_resources_read(request: RpcRequest) -> RpcResponse {
    let uri = request
        .params
        .get("uri")
        .and_then(Value::as_str)
        .unwrap_or("");

    if uri != "toastmcp://assets" {
        return error_response(
            request.id.unwrap_or(Value::Null),
            -32602,
            format!("Unknown resource: {uri}"),
        );
    }

    let icons = list_icon_ids();
    let sounds = list_sound_ids();
    let payload = serde_json::json!({
        "icons": icons,
        "sounds": sounds
    })
    .to_string();

    ok_response(
        request,
        serde_json::json!({
            "contents": [{
                "uri": "toastmcp://assets",
                "mimeType": "application/json",
                "text": payload
            }]
        }),
    )
}

fn ok_response(request: RpcRequest, result: Value) -> RpcResponse {
    RpcResponse {
        jsonrpc: "2.0",
        id: request.id.unwrap_or(Value::Null),
        result: Some(result),
        error: None,
    }
}

fn error_response(id: Value, code: i64, message: String) -> RpcResponse {
    RpcResponse {
        jsonrpc: "2.0",
        id,
        result: None,
        error: Some(RpcError { code, message }),
    }
}

#[derive(Copy, Clone, Debug)]
enum Framing {
    Lsp,
    JsonLine,
}

struct IncomingMessage {
    payload: String,
    framing: Framing,
}

fn read_message(reader: &mut impl BufRead) -> Result<Option<IncomingMessage>> {
    let mut content_length: Option<usize> = None;
    let mut line = String::new();

    loop {
        line.clear();
        let bytes = reader.read_line(&mut line)?;
        if bytes == 0 {
            return Ok(None);
        }
        let trimmed = line.trim_end_matches(['\r', '\n']);
        if trimmed.starts_with('{') && trimmed.contains("\"jsonrpc\"") {
            return Ok(Some(IncomingMessage {
                payload: trimmed.to_string(),
                framing: Framing::JsonLine,
            }));
        }
        if trimmed.is_empty() {
            break;
        }
        if let Some((name, value)) = trimmed.split_once(':') {
            if name.trim().eq_ignore_ascii_case("content-length") {
                content_length = Some(
                    value
                        .trim()
                        .parse::<usize>()
                        .context("Invalid Content-Length header")?,
                );
            }
        }
    }

    let length = content_length.ok_or_else(|| anyhow!("Missing Content-Length header"))?;
    let mut buf = vec![0u8; length];
    reader.read_exact(&mut buf)?;
    let payload = String::from_utf8(buf).context("Payload is not valid UTF-8")?;
    Ok(Some(IncomingMessage {
        payload,
        framing: Framing::Lsp,
    }))
}


fn write_message(writer: &mut impl Write, response: &RpcResponse, framing: Framing) -> Result<()> {
    let payload = serde_json::to_string(response)?;
    match framing {
        Framing::Lsp => {
            write!(writer, "Content-Length: {}\r\n\r\n", payload.len())?;
            writer.write_all(payload.as_bytes())?;
        }
        Framing::JsonLine => {
            writer.write_all(payload.as_bytes())?;
            writer.write_all(b"\n")?;
        }
    }
    writer.flush()?;
    Ok(())
}
