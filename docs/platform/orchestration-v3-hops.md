# Orchestration V3: ordered hop chains

V2 intentionally has one contract:

```text
rule -> node_group | builtin(direct)
```

`GroupSource::Group` is a reusable/combined source. Its members are flattened
into one outbound during publication. It is therefore parallel selection, not
serial forwarding. A V2 graph must not present that flattening as source-hiding
multi-hop support.

The requested “forward through A, then B, then C” behavior needs a separate
data-plane capability and document version. The proposed V3 shape is:

```text
rule -> chain
chain.hops = [endpoint | subscription | combined_group, ...]
chain.on_failure = reject | direct | next_available
```

The compiler should retain an ordered `CompiledHopChain` in a backend-neutral
IR. Each hop must be resolved to an immutable node identity at publication;
missing nodes, cycles, duplicate hops, and unsupported protocols fail closed.
The Linux and Windows renderers then advertise capabilities independently:

- Linux: only enable after a real engine can enforce ordered proxy dialing and
  DNS/UDP behavior; dae's current group renderer is not sufficient evidence.
- Windows: render to the selected sing-box/mihomo engine together with Wintun;
  never synthesize chains from edge conditions or pretend a flattened group is
  serial.

The UI should add chains as an ordered list inside the inspector, with explicit
add/remove/reorder actions, a hop preview, and a capability/error panel. The
canvas remains a rule-to-chain topology; hop order is edited in the inspector,
not by adding legacy branch/start/end nodes.

Acceptance gates before enabling V3 publication:

1. Linux and Windows renderers agree on the IR semantics for TCP, UDP, DNS,
   IPv4, and IPv6.
2. A network test proves the destination sees the final hop and cannot observe
   the original endpoint when the chain is enabled.
3. Crash/reload tests restore routes, DNS, and engine processes without leaking
   traffic through direct.
4. V2 documents remain readable and publishable with unchanged semantics.
