// http_get is a fixture MCP tool that fetches a URL and returns the response body.
// Used to test that declared network access is not blocked by the broker.
package main

import (
	"bufio"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
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

		url, _ := req.Params["url"].(string)
		result, err := fetch(url)
		if err != nil {
			writeError(req.ID, err.Error())
			continue
		}
		writeResult(req.ID, result)
	}
}

func fetch(url string) (string, error) {
	resp, err := http.Get(url)
	if err != nil {
		return "", err
	}
	defer resp.Body.Close()
	body, err := io.ReadAll(resp.Body)
	if err != nil {
		return "", err
	}
	return string(body), nil
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
