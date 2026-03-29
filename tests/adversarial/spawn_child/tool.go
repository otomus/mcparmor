// Adversarial MCP tool: spawn_child
//
// Exposes a run_command tool that attempts to spawn a child process (/bin/sh)
// and return its output. This tests OS-level spawn blocking:
//
//   - macOS (Seatbelt, spawn: false): (deny process-exec) in the SBPL profile
//     prevents exec() from succeeding — the tool returns SPAWN_BLOCKED.
//   - Linux (Landlock, spawn: false): Layer 2 spawn blocking is not yet
//     implemented (documented in enforcement_summary). The tool successfully
//     spawns the child and returns SPAWN_SUCCESS. The test records this as
//     INFORMATIONAL rather than a CI failure.
//
// In both cases the broker does not intercept this at Layer 1 (no path or URL
// argument is sent), so this test exercises Layer 2 only.
package main

import (
	"bufio"
	"encoding/json"
	"fmt"
	"os"
	"os/exec"
	"strings"
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

func trySpawnChild() string {
	// Use a simple, deterministic command so the output is easy to check.
	cmd := exec.Command("/bin/sh", "-c", "echo SPAWNED")
	out, err := cmd.Output()
	if err != nil {
		return "SPAWN_BLOCKED: " + err.Error()
	}
	return "SPAWN_SUCCESS: " + strings.TrimSpace(string(out))
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
					"serverInfo":      map[string]interface{}{"name": "spawn-child-tool", "version": "1.0"},
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
							"name":        "run_command",
							"description": "Run a shell command and return its output",
							"inputSchema": map[string]interface{}{
								"type":       "object",
								"properties": map[string]interface{}{},
							},
						},
					},
				},
			})

		case "tools/call":
			result := trySpawnChild()
			send(response{
				JSONRPC: "2.0",
				ID:      req.ID,
				Result: map[string]interface{}{
					"content": []interface{}{
						map[string]interface{}{
							"type": "text",
							"text": result,
						},
					},
				},
			})
		}
	}
}
