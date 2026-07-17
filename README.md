# chaos

Modern control plane for [dae](https://github.com/daeuniverse/dae): **Rust API + SvelteKit UI + vendored dae**.

> Status: design phase. See [product design](docs/superpowers/specs/2026-07-18-chaos-product-design.md).

## Goals

- Full replacement for daed as a product experience
- Single install ships `chaos` and `dae` (no separate dae/daed/dae-wing required)
- REST JSON API; SvelteKit console

## Layout (planned)

```text
apps/web          SvelteKit
crates/chaos-api  REST server
crates/chaos-core domain + latency
crates/chaos-dae  dae binary integration
crates/chaos-store SQLite
```

## Development

Scaffolding and run instructions will land with the first implementation plan.

Default API port target: **2030** (avoids system daed on **2023**).

## License

TBD at first code drop (likely MIT and/or AGPL alignment with dae components—decide before release).
