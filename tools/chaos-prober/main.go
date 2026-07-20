// chaos-prober — real proxy latency prober via daeuniverse/outbound.
//
// Usage:
//
//	echo '{"links":["vless://..."],"url":"http://cp.cloudflare.com","timeout_ms":5000}' | ./chaos-prober
//	./chaos-prober --link "vless://..." --url "http://cp.cloudflare.com"
package main

import (
	"encoding/json"
	"fmt"
	"os"
)

// ProbeRequest is the JSON request from stdin.
type ProbeRequest struct {
	Links       []string `json:"links"`
	URL         string   `json:"url,omitempty"`
	TimeoutMs   int      `json:"timeout_ms,omitempty"`
	Concurrency int      `json:"concurrency,omitempty"`
}

// ProbeResponse is the JSON response to stdout.
type ProbeResponse struct {
	Results []ProbeResult `json:"results"`
}

// ProbeResult holds the latency test result for one link.
type ProbeResult struct {
	Link      string  `json:"link"`
	LatencyMs *int64  `json:"latency_ms"`
	Alive     bool    `json:"alive"`
	Error     string  `json:"error,omitempty"`
}

func main() {
	// CLI mode: --link "..." [--url "..."]
	if hasCLIFlag("--link") {
		link := getCLIFlag("--link")
		if link == "" {
			fatal("--link requires a value")
		}
		url := getCLIFlag("--url")
		if url == "" {
			url = defaultCheckURL
		}
		timeoutMs := 5000
		result := probeSingle(link, url, timeoutMs)
		out, _ := json.MarshalIndent(result, "", "  ")
		fmt.Println(string(out))
		if !result.Alive {
			os.Exit(1)
		}
		return
	}

	// Stdin JSON mode
	var req ProbeRequest
	if err := json.NewDecoder(os.Stdin).Decode(&req); err != nil {
		fatal("invalid JSON input: " + err.Error())
	}
	if len(req.Links) == 0 {
		fatal("no links provided")
	}
	if req.URL == "" {
		req.URL = defaultCheckURL
	}
	if req.TimeoutMs <= 0 {
		req.TimeoutMs = 5000
	}
	if req.Concurrency <= 0 {
		req.Concurrency = 20
	}

	results := probeBatch(req.Links, req.URL, req.TimeoutMs, req.Concurrency)
	resp := ProbeResponse{Results: results}
	enc := json.NewEncoder(os.Stdout)
	enc.SetIndent("", "  ")
	if err := enc.Encode(resp); err != nil {
		fatal("encode response: " + err.Error())
	}
}

func hasCLIFlag(flag string) bool {
	for _, arg := range os.Args[1:] {
		if arg == flag {
			return true
		}
	}
	return false
}

func getCLIFlag(flag string) string {
	for i, arg := range os.Args[1:] {
		if arg == flag && i+1 < len(os.Args)-1 {
			return os.Args[i+2]
		}
	}
	return ""
}

func fatal(msg string) {
	fmt.Fprintf(os.Stderr, "chaos-prober: %s\n", msg)
	os.Exit(2)
}
