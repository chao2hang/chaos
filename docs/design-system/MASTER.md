# chaos Interface System

**Status:** Active
**Scope:** SvelteKit console
**Source of truth:** `apps/web/src/app.css` and `apps/web/src/lib/components/`

## Direction

chaos is an operational control plane. The interface is quiet, compact, and explicit.

- True white canvas, near-black text, and neutral grays only.
- No brand accent color, glow, decorative gradient, glass effect, or floating card grid.
- Borders and spacing establish hierarchy. Shadows are reserved for modal layers and the mobile drawer.
- System UI typography is used for reading speed. Monospace is limited to identifiers, paths, addresses, and expressions.
- Status is never communicated by color alone. Every state has an icon and text.
- Radius is capped at 6px. Repeated data rows may be framed; page sections are not decorative cards.
- Controls remain stable in size during hover, loading, validation, and localization.

## Tokens

| Role | Token | Value |
|---|---|---|
| Canvas | `--canvas` | `#ffffff` |
| Surface | `--surface` | `#ffffff` |
| Subtle surface | `--surface-subtle` | `#f5f5f5` |
| Hover surface | `--surface-hover` | `#eeeeee` |
| Inverse surface | `--surface-inverse` | `#111111` |
| Primary text | `--ink` | `#111111` |
| Muted text | `--ink-muted` | `#5f5f5f` |
| Faint text | `--ink-faint` | `#858585` |
| Border | `--line` | `#dddddd` |
| Strong border | `--line-strong` | `#a8a8a8` |
| Focus | `--focus` | `#111111` |
| Small radius | `--radius-sm` | `3px` |
| Default radius | `--radius-md` | `5px` |
| Maximum radius | `--radius-lg` | `6px` |
| Sidebar | `--sidebar-width` | `15rem` |
| Content limit | `--content-width` | `76rem` |

Spacing follows a 4px base through `--space-1` to `--space-12`.

## Typography

- UI: system sans stack from `--font-ui`.
- Technical data: system monospace stack from `--font-mono`.
- Page title: 1.65rem / 720 weight.
- Section title: 0.9rem / 690 weight.
- Control text: 0.8rem / 650 weight.
- Labels: 0.68-0.75rem / 650 weight.
- Letter spacing is zero. Uppercase is restricted to short navigation and technical labels.

## Layout

- Desktop uses a 15rem fixed sidebar and a centered 76rem content area.
- Mobile uses a 3.5rem sticky top bar and an off-canvas navigation drawer.
- Pages use `.page-stack` for vertical rhythm and `.page-grid` for 12-column composition.
- Data tables collapse into labeled row blocks below 720px.
- Sticky save bars remain above the viewport edge and stack their actions on narrow screens.
- No section may depend on horizontal scrolling except a desktop data table as a last-resort overflow container.

## Component Contracts

| Component | Purpose | Required states |
|---|---|---|
| `AppLogo` | Product mark and wordmark | Full, compact |
| `ActionLink` | Link styled as a command | Primary, secondary, ghost, small |
| `AuthShell` | Shared login/setup frame | Loading, form, API error |
| `Button` | All button commands | Primary, secondary, ghost, danger, loading, disabled, icon-only |
| `ConfirmDialog` | Destructive or discard confirmation | Open, busy, cancel, confirm |
| `EmptyState` | Empty resource or filtered result | Icon, title, description, optional action |
| `Field` | Label, control, hint, and validation ownership | Default, optional, hint, invalid |
| `LoadingState` | Section or page loading feedback | Label and reduced-motion-safe spinner |
| `Metric` | Dashboard summary value | Positive, negative, neutral |
| `Notice` | Inline feedback | Info, success, error, dismissible |
| `PageHeader` | Page identity and primary commands | Metadata, title, description, actions |
| `PasswordInput` | Password entry with visibility control | Hidden, visible, disabled |
| `SearchInput` | Resource filtering | Empty, populated, clear, disabled |
| `Section` | Bordered functional region | Header, actions, count, flush body |
| `SegmentedControl` | Small mutually exclusive view switch | Selected, unselected, counted |
| `Status` | Textual operational state | Positive, negative, warning, neutral |
| `TableFrame` | Stable table boundary and overflow | Desktop table, responsive row blocks |
| `Toggle` | Binary setting | On, off, focused, disabled |

## Feature Components

| Component | Ownership |
|---|---|
| `RoutingRuleEditor` | Ordered rule creation, enablement, duplication, movement, target selection, and deletion |
| `NamedEndpointEditor` | Ordered DNS upstream names and addresses |
| `GroupDraftEditor` | Group selection, policy fields, multi-group node membership, and weights |
| `FlowPreview` | Read-only topology verification using `@xyflow/svelte` |

Feature components must use the UI primitives above. They do not introduce independent palettes, button styles, notices, or confirmation behavior.

## Interaction Rules

### Async operations

- Loading state is scoped to the affected control or row when possible.
- A row refresh must not disable unrelated rows.
- Success and failure feedback use `Notice` and remain dismissible.
- User input is preserved after a failed request.
- A bulk import keeps failed links in the editor for correction and retry.

### Drafts and saving

- Routing, DNS, group membership, and orchestration use local drafts.
- Dirty state is visible in both the page header context and sticky save bar.
- Reloading a dirty document requires confirmation.
- Browser unload protection is enabled while a draft is dirty.
- Validation runs before persistence and before Apply.
- Orchestration saves group drafts first, then routing, then optionally applies the runtime configuration.

### Destructive actions

- Native `confirm()` is not used.
- Delete and stop actions use `ConfirmDialog` with the affected resource named in the copy.
- Deleting a referenced group assigns its rule references to an explicit replacement and leaves routing dirty for review.

### Selection and editing

- Dragging is never the only way to edit.
- Nodes can belong to multiple groups.
- Resource tables expose row actions as icon buttons with accessible labels and tooltips.
- Search filters locally and communicates an empty-filter result separately from an empty resource.
- Ordered rules always provide keyboard-accessible move buttons.

### Orchestration

The workflow has three stages:

1. Groups and members.
2. Ordered routing rules and fallback.
3. Validation, topology review, save, and Apply.

The topology canvas is a read-only verification surface. It supports pan and zoom but does not imply that moving or connecting nodes persists configuration.

## Copy

- Use direct labels: "Save changes", "Apply configuration", "Delete node".
- Avoid slogans, ornamental technical jargon, fake metrics, and explanatory feature copy.
- Buttons describe commands, not destinations or benefits.
- Errors explain what failed and retain server detail for dae operational failures.
- English and Simplified Chinese catalogs must contain identical keys.

## Accessibility

- Every icon-only control has an `aria-label` and `title`.
- Focus uses a visible 2px near-black outline.
- Interactive targets are at least 30px high; primary controls are at least 36px high.
- Status includes text and an icon, never color alone.
- Modal layers support Escape and backdrop cancellation when not busy.
- `prefers-reduced-motion` reduces animation and transition durations.
- Contrast must meet WCAG AA for normal text.

## Verification

Before handoff:

- Run `pnpm --dir apps/web check`.
- Run `pnpm --dir apps/web build`.
- Verify 1440px, 1024px, 768px, and 375px viewports.
- Exercise login/setup, resource import, row refresh/test, delete confirmation, draft discard, orchestration save, and save-and-apply.
- Check for untranslated keys, horizontal overflow, clipped labels, and controls that resize during loading.
- Scan source colors: only neutral grayscale values are allowed.
