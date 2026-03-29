// Adversarial MCP tool: path_traversal
//
// Exposes a read_file tool. The test runner calls it with a path traversal
// argument ("../../etc/passwd"). The broker must block this at Layer 1 before
// the tool ever receives the tools/call message.
package main

import (
	"bufio"
	"encoding/json"
	"fmt"
	"os"
)

type request struct {
	JSONRPC string          `json:"jsonrpc"`
	ID      interface{}     `json:"id,omitempty"`
	Method  string          `json:"method"`
	Params  json.RawMessage `json:"params,omitempty"`
}

type response struct {
	JSONRPC string      `json:"jsonrpc"`
	ID      interface{} `json:"id"`
	Result  interface{} `json:"result,omitempty"`
}

func send(v interface{}) {
	b, _ := json.Marshal(v)
	fmt.Fprintln(os.Stdout, string(b))
}

func main() {
	scanner := bufio.NewScanner(os.Stdin)
	scanner.Buffer(make([]byte, 1<<20), 1<<20)

	for scanner.Scan() {
		line := scanner.Text()
		if line == "" {
			continue
		}

		var req request
		if err := json.Unmarshal([]byte(line), &req); err != nil {
			continue
		}

		switch req.Method {
		case "initialize":
			send(response{
				JSONRPC: "2.0",
				ID:      req.ID,
				Result: map[string]interface{}{
					"protocolVersion": "2024-11-05",
					"capabilities":    map[string]interface{}{},
					"serverInfo":      map[string]interface{}{"name": "path-traversal-tool", "version": "1.0"},
				},
			})

		case "notifications/initialized":
			// Notification — no response.

		case "tools/list":
			send(response{
				JSONRPC: "2.0",
				ID:      req.ID,
				Result: map[string]interface{}{
					"tools": []interface{}{
						map[string]interface{}{
							"name":        "read_file",
							"description": "Read a file from the filesystem",
							"inputSchema": map[string]interface{}{
								"type": "object",
								"properties": map[string]interface{}{
									"path": map[string]interface{}{"type": "string"},
								},
								"required": []string{"path"},
							},
						},
					},
				},
			})

		case "tools/call":
			// The broker must never let this reach us when called with a
			// traversal path. If it does, the test fails.
			var params struct {
				Name      string          `json:"name"`
				Arguments json.RawMessage `json:"arguments"`
			}
			_ = json.Unmarshal(req.Params, &params)
			send(response{
				JSONRPC: "2.0",
				ID:      req.ID,
				Result: map[string]interface{}{
					"content": []interface{}{
						map[string]interface{}{
							"type": "text",
							"text": "REACHED_TOOL: call was not blocked by broker",
						},
					},
				},
			})
		}
	}
}
