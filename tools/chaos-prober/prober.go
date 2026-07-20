package main

import (
	"context"
	"net"
	"net/http"
	"sync"
	"time"

	"github.com/daeuniverse/outbound/dialer"
	"github.com/daeuniverse/outbound/netproxy"
	"github.com/daeuniverse/outbound/protocol/direct"

	// Blank imports: dialer layer (link parsing + transport registration).
	_ "github.com/daeuniverse/outbound/dialer/hysteria2"
	_ "github.com/daeuniverse/outbound/dialer/shadowsocks"
	_ "github.com/daeuniverse/outbound/dialer/trojan"
	_ "github.com/daeuniverse/outbound/dialer/v2ray"
	_ "github.com/daeuniverse/outbound/dialer/socks"
	_ "github.com/daeuniverse/outbound/dialer/http"
	_ "github.com/daeuniverse/outbound/dialer/tuic"
	_ "github.com/daeuniverse/outbound/dialer/juicity"
	_ "github.com/daeuniverse/outbound/dialer/anytls"
	_ "github.com/daeuniverse/outbound/dialer/shadowsocksr"

	// Blank imports: protocol layer (connection creators).
	_ "github.com/daeuniverse/outbound/protocol/vless"
	_ "github.com/daeuniverse/outbound/protocol/vmess"
	_ "github.com/daeuniverse/outbound/protocol/trojanc"
	_ "github.com/daeuniverse/outbound/protocol/shadowsocks"
	_ "github.com/daeuniverse/outbound/protocol/shadowsocks_stream"
	_ "github.com/daeuniverse/outbound/protocol/hysteria2"
	_ "github.com/daeuniverse/outbound/protocol/tuic"
	_ "github.com/daeuniverse/outbound/protocol/juicity"
	_ "github.com/daeuniverse/outbound/protocol/anytls"

	// Blank imports: transport layer.
	_ "github.com/daeuniverse/outbound/transport/tls"
	_ "github.com/daeuniverse/outbound/transport/ws"
	_ "github.com/daeuniverse/outbound/transport/simpleobfs"
)

const defaultCheckURL = "http://cp.cloudflare.com"

func init() {
	direct.InitDirectDialers("")
}

func probeSingle(link, url string, timeoutMs int) ProbeResult {
	d, _, err := dialer.NewNetproxyDialerFromLink(
		direct.SymmetricDirect,
		&dialer.ExtraOption{},
		link,
	)
	if err != nil {
		return ProbeResult{
			Link:  link,
			Alive: false,
			Error: "parse link: " + err.Error(),
		}
	}

	transport := &http.Transport{
		DialContext: func(ctx context.Context, network, addr string) (net.Conn, error) {
			c, err := d.DialContext(ctx, network, addr)
			if err != nil {
				return nil, err
			}
			return &netproxy.FakeNetConn{Conn: c}, nil
		},
		DisableKeepAlives: true,
	}

	client := &http.Client{
		Transport: transport,
		Timeout:   time.Duration(timeoutMs) * time.Millisecond,
		// Don't follow redirects — we only care about the first response.
		CheckRedirect: func(req *http.Request, via []*http.Request) error {
			return http.ErrUseLastResponse
		},
	}

	req, err := http.NewRequest("HEAD", url, nil)
	if err != nil {
		return ProbeResult{
			Link:  link,
			Alive: false,
			Error: "build request: " + err.Error(),
		}
	}

	start := time.Now()
	resp, err := client.Do(req)
	elapsed := time.Since(start).Milliseconds()

	if err != nil {
		return ProbeResult{
			Link:  link,
			Alive: false,
			Error: err.Error(),
		}
	}
	resp.Body.Close()

	return ProbeResult{
		Link:      link,
		LatencyMs: &elapsed,
		Alive:     true,
	}
}

func probeBatch(links []string, url string, timeoutMs, concurrency int) []ProbeResult {
	results := make([]ProbeResult, len(links))
	sem := make(chan struct{}, concurrency)
	var wg sync.WaitGroup

	for i, link := range links {
		wg.Add(1)
		sem <- struct{}{} // acquire
		go func(idx int, lnk string) {
			defer wg.Done()
			defer func() { <-sem }() // release
			results[idx] = probeSingle(lnk, url, timeoutMs)
		}(i, link)
	}
	wg.Wait()
	return results
}
