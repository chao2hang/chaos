# Orchestration API

All endpoints are under `/api/v1`.

## Authentication

Interactive users may use the existing 24-hour JWT. Automation should use an API key:

```http
Authorization: Bearer chaos_sk_<secret>
```

An administrator creates a key with `POST /api/v1/auth/api-keys`. The complete secret is returned once; Chaos stores only a SHA-256 hash. The default key is read/validate/simulate/plan only. Add `orchestration:publish` only for a controlled deployment agent.

## Safe workflow

1. `GET /api/v1/orchestration/capabilities`
2. `POST /api/v1/orchestration/validate` with the document.
3. `POST /api/v1/orchestration/plan` with `{ "document": {}, "allow_degraded_groups": false }`.
4. `POST /api/v1/orchestration/simulate` with `{ "document": {}, "probe": { "domain": "example.com" } }`.
5. Save a draft with `PUT /api/v1/orchestration` (administrator only).
6. Publish only after inspecting the plan. Publishing also applies the runtime and rolls back on failed dataplane verification.

`plan` expands node, subscription and nested-group sources and reports member health, redundancy, dead-only groups, and the effective fallback. A group with zero known healthy members is rejected during publish. A group with one healthy member is publishable but reported as degraded.

## API key administration

```bash
curl -X POST http://host:2030/api/v1/auth/api-keys \
  -H "Authorization: Bearer <admin-jwt>" -H 'Content-Type: application/json' \
  -d '{"name":"orchestration-agent"}'
curl http://host:2030/api/v1/auth/api-keys -H "Authorization: Bearer <admin-jwt>"
curl -X DELETE http://host:2030/api/v1/auth/api-keys/<id> -H "Authorization: Bearer <admin-jwt>"
```

## External orchestration examples

```bash
curl http://host:2030/api/v1/orchestration/capabilities -H "Authorization: Bearer $CHAOS_API_KEY"
curl -X POST http://host:2030/api/v1/orchestration/validate \
  -H "Authorization: Bearer $CHAOS_API_KEY" -H 'Content-Type: application/json' --data @document.json
curl -X POST http://host:2030/api/v1/orchestration/plan \
  -H "Authorization: Bearer $CHAOS_API_KEY" -H 'Content-Type: application/json' \
  -d '{"document":{},"allow_degraded_groups":false}'
```

The existing `/orchestration`, `/orchestration/validate`, `/orchestration/simulate`, and `/orchestration/publish` routes remain compatible. API keys are accepted there with the corresponding scope; publish still requires an administrator-owned key and the explicit publish scope.
