# Orchestration templates

These templates are intentionally resource-neutral. Replace `{{...}}` values with IDs from `GET /api/v1/nodes`, `GET /api/v1/subscriptions`, or `GET /api/v1/groups` before validation. Do not invent UUIDs.

The graph invariant is:

```text
start -> every rule -> one terminal (node group or direct)
end -> exactly one terminal fallback
```

Rules are first-match-wins. Chaos orders them by matcher band (domain, geosite, IP/GeoIP, other), then by unique priority, then node ID. Keep priorities unique even across different matcher kinds.

Recommended AI generation contract:

1. Discover resources first and use their exact IDs.
2. Keep at least two independently healthy nodes in every proxy group intended for failover.
3. Validate, plan, and simulate before saving or publishing.
4. Treat `group_all_members_dead` as a hard failure and `group_no_redundancy` as a deployment warning.
5. Never use `PUT` or `publish` as the first call from an agent.

Files:

- `minimal-safe-default.json`: domestic direct, everything else through one proxy group.
- `service-specific-routing.json`: service domains through a dedicated group, fallback through another group.
- `multi-group-fallback.json`: primary group first, explicit backup group for selected traffic, direct fallback.
