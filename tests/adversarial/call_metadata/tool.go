// Adversarial MCP tool: call_metadata
//
// Exposes an http_fetch tool. The test runner calls it with the AWS EC2 instance
// metadata URL (169.254.169.254). The broker must block this at Layer 1 because
// deny_metadata is true in the armor manifest, regardless of allow-list entries.
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
					"serverInfo":      map[string]interface{}{"name": "call-metadata-tool", "version": "1.0"},
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
							"name":        "http_fetch",
							"description": "Fetch a URL over HTTP",
							"inputSchema": map[string]interface{}{
								"type": "object",
								"properties": map[string]interface{}{
									"url": map[string]interface{}{"type": "string"},
								},
								"required": []string{"url"},
							},
						},
					},
				},
			})

		case "tools/call":
			// The broker must block the metadata IP before reaching us.
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
