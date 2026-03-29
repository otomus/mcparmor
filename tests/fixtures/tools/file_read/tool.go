// file_read is a fixture MCP tool that reads files from /tmp/mcparmor/ only.
// Used to test that declared filesystem access is not blocked by the broker.
package main

import (
	"bufio"
	"encoding/json"
	"fmt"
	"os"
)

type Request struct {
	JSONRPC string                 `json:"jsonrpc"`
	ID      interface{}            `json:"id"`
	Method  string                 `json:"method"`
	Params  map[string]interface{} `json:"params,omitempty"`
}

type Response struct {
	JSONRPC string      `json:"jsonrpc"`
	ID      interface{} `json:"id"`
	Result  interface{} `json:"result,omitempty"`
	Error   interface{} `json:"error,omitempty"`
}

func main() {
	scanner := bufio.NewScanner(os.Stdin)
	for scanner.Scan() {
		var req Request
		if err := json.Unmarshal(scanner.Bytes(), &req); err != nil {
			continue
		}

		path, _ := req.Params["path"].(string)
		content, err := os.ReadFile(path)
		if err != nil {
			writeError(req.ID, err.Error())
			continue
		}
		writeResult(req.ID, string(content))
	}
}

func writeResult(id interface{}, result interface{}) {
	resp := Response{JSONRPC: "2.0", ID: id, Result: result}
	out, _ := json.Marshal(resp)
	fmt.Println(string(out))
}

func writeError(id interface{}, msg string) {
	resp := Response{JSONRPC: "2.0", ID: id, Error: map[string]interface{}{"code": -32000, "message": msg}}
	out, _ := json.Marshal(resp)
	fmt.Println(string(out))
}
