"""Minimal MCP stdio example. stdout is exclusively newline-delimited JSON-RPC."""
import json
import sys


TOOLS = [
    {
        "name": "echo",
        "description": "Return the supplied message unchanged; no side effects.",
        "inputSchema": {
            "type": "object",
            "properties": {"message": {"type": "string"}},
            "required": ["message"],
            "additionalProperties": False,
        },
    },
    *[
        {
            "name": name,
            "description": "Acknowledge a Zeus lifecycle event without side effects.",
            "inputSchema": {"type": "object"},
        }
        for name in ("on_start", "on_end", "on_error")
    ],
]


def handle(request):
    method = request["method"]
    if method == "initialize":
        return {
            "protocolVersion": "2024-11-05",
            "capabilities": {"tools": {}},
            "serverInfo": {"name": "zeus-echo-example", "version": "1.0.0"},
        }
    if method == "tools/list":
        return {"tools": TOOLS}
    if method != "tools/call":
        raise ValueError("unsupported method")
    params = request["params"]
    name, arguments = params["name"], params["arguments"]
    if not isinstance(arguments, dict):
        raise ValueError("arguments must be an object")
    if name == "echo":
        if set(arguments) != {"message"} or not isinstance(arguments["message"], str):
            return {"isError": True, "content": [{"type": "text", "text": "message must be a string"}]}
        text = arguments["message"]
    elif name in ("on_start", "on_end", "on_error"):
        text = "Lifecycle event acknowledged"
    else:
        raise ValueError("unknown tool")
    return {"content": [{"type": "text", "text": text}]}


def main():
    for line in sys.stdin:
        request = None
        try:
            request = json.loads(line)
            if not isinstance(request, dict):
                raise ValueError("request must be an object")
            if "id" not in request:
                continue
            response = {"jsonrpc": "2.0", "id": request["id"], "result": handle(request)}
        except (ValueError, KeyError, TypeError):
            response = {
                "jsonrpc": "2.0",
                "id": request.get("id") if isinstance(request, dict) else None,
                "error": {"code": -32602, "message": "Invalid request or arguments"},
            }
        print(json.dumps(response), flush=True)


if __name__ == "__main__":
    main()
