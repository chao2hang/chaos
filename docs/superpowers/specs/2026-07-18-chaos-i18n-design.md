# chaos i18n Design

**Date:** 2026-07-18  
**Status:** Approved for implementation  
**Branch:** `feat/mvp`  
**Scope:** Full-stack shared locale catalog (Web + API errors; future CLI-ready)

## Goals

- Single source of truth for user-visible strings: monorepo root `locales/`.
- First languages: **`en`** (source + fallback) and **`zh-CN`**.
- Architecture accepts more locales without redesign.
- Web UI, API `error.message`, and future CLI share the same key space.
- Logs / tracing stay English only.

## Non-goals

- Account-persisted language preference (localStorage only for MVP).
- URL-prefixed locales (`/zh/dashboard`).
- Fluent/ICU full plural rules.
- Localized operator logs.
- Auto-translation pipeline.
- Routing / groups / DNS product features (separate epic; see product design P3/P4).

## Approach

**Shared JSON catalogs + dual light loaders** (not Fluent, not dual-source Paraglide).

| Layer | Mechanism |
|-------|-----------|
| Catalog | `locales/en.json`, `locales/zh-CN.json` (flat dotted keys) |
| Rust | `chaos-i18n` crate embeds JSON via `include_str!` |
| Web | Import same JSON via Vite alias `$locales` |
| API | `Accept-Language` → locale; `error.message` from `error.{code}` |
| Web UX | Browser languages → `localStorage.chaos_locale` → manual switcher |

## Locale IDs and normalization

| ID | Switcher label |
|----|----------------|
| `en` | English |
| `zh-CN` | 简体中文 |

Rules:

- `zh`, `zh-Hans`, `zh-CN`, `zh-SG` → `zh-CN`
- `en`, `en-US`, `en-GB`, … → `en`
- Unknown → `en`

## Key conventions

Flat object, dotted namespaces:

- `nav.*`, `auth.*`, `dashboard.*`, `nodes.*`, `subscriptions.*`, `common.*`
- `error.*` — suffix matches API `error.code` (e.g. `unauthorized` → `error.unauthorized`)

Interpolation: `{name}`, `{count}` only (simple replace, both sides identical).

Missing key: try active locale → `en` → display the key string (dev-friendly).

## Language resolution

### Web (first visit)

1. `localStorage.chaos_locale` if valid  
2. `navigator.languages` / `navigator.language` (normalized)  
3. Fallback `en`

On switch: update store, persist localStorage, set `document.documentElement.lang`, re-render; no full reload.

### Web → API

Every API `fetch` sends:

```http
Accept-Language: zh-CN
```

(single normalized tag.)

### API

1. Parse `Accept-Language` (first recognizable tag)  
2. Else `en`  
3. Build `ApiError.message` via catalog `error.{code}` when using coded constructors  
4. Success JSON field **names** stay English; only user-visible messages localize  

No JWT / DB language field in MVP.

## Components

### `crates/chaos-i18n`

- `Locale` enum + parse / Accept-Language helpers  
- `t(locale, key, params)` / `error_message(locale, code)`  
- Unit tests: normalization, fallback, interpolation  

### `chaos-api`

- `RequestLocale` extractor  
- `ApiError` coded constructors resolve message from catalog  
- Import-row errors (`invalid_link`, etc.) use the same catalog  
- Internal details stay in logs; client `internal_error` message is generic localized text  

### `apps/web`

- `$lib/i18n` — locale store, `t()`, `initLocale()`, `setLocale()`  
- `LocaleSwitcher` on app chrome + login/setup  
- All route copy via `t()`  
- `api()` attaches `Accept-Language`  
- Prefer `t('error.' + code)` when showing API errors; else `message`  

## Testing

- Rust: `chaos-i18n` unit tests; API tests with `Accept-Language: zh-CN` assert Chinese message for a known code; default still English  
- Web: `pnpm check` / `build` green  
- Optional: script later to assert en/zh-CN key sets equal  

## Success criteria

1. Language switch updates nav, forms, empty states, confirms immediately.  
2. API errors localize by `Accept-Language`; `code` stable.  
3. New language = new JSON + register in Web + Rust lists.  
4. Existing workspace tests and web check/build pass.  

## Out of scope follow-up (user request)

**Routing / groups / DNS (daed-parity)** is **not** this spec. Track as product phases **P3 (groups/policies)** and **P4 (config/routing/DNS editors)** from `2026-07-18-chaos-product-design.md`. Implement only after a dedicated design/plan.

## Implementation notes

- Embed path from `crates/chaos-i18n/src`: `../../../locales/*.json`  
- Vite alias: `$locales` → repo `locales/`  
- Keep English catalog text aligned with pre-i18n strings so default tests remain stable  
