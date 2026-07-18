<script lang="ts">
	import { onMount } from 'svelte';
	import {
		listNodes,
		listGroups,
		createGroup,
		addGroupMember,
		removeGroupMember,
		setGroupMemberWeight,
		getRouting,
		putRouting,
		ApiClientError,
		type NodeDto,
		type GroupDto,
		type RoutingRuleDto
	} from '$lib/api';
	import { apiErrorText, t } from '$lib/i18n.svelte';

	let nodes = $state<NodeDto[]>([]);
	let groups = $state<GroupDto[]>([]);
	let rules = $state<RoutingRuleDto[]>([]);
	let fallback = $state('proxy');
	let error = $state('');
	let message = $state('');
	let busy = $state(false);
	let dragNodeId = $state<string | null>(null);
	let dragOverGroup = $state<string | null>(null);
	let newGroupName = $state('');
	let newPolicy = $state('fixed');

	const memberIds = $derived.by(() => {
		const s = new Set<string>();
		for (const g of groups) for (const m of g.members ?? []) s.add(m.node_id);
		return s;
	});

	const freeNodes = $derived(nodes.filter((n) => !memberIds.has(n.id)));

	async function reload() {
		error = '';
		try {
			const [n, g, r] = await Promise.all([listNodes(), listGroups(), getRouting()]);
			nodes = n.nodes;
			groups = g.groups.map((x) => ({ ...x, members: x.members ?? [] }));
			rules = r.rules;
			fallback = r.fallback;
		} catch (e) {
			error = e instanceof ApiClientError ? apiErrorText(e) : t('groups.loadFailed');
		}
	}

	onMount(() => {
		void reload();
	});

	function onDragStart(id: string, e: DragEvent) {
		dragNodeId = id;
		e.dataTransfer?.setData('text/plain', id);
		if (e.dataTransfer) e.dataTransfer.effectAllowed = 'copyMove';
	}

	function onDragEnd() {
		dragNodeId = null;
		dragOverGroup = null;
	}

	async function dropOnGroup(groupId: string, e: DragEvent) {
		e.preventDefault();
		const id = e.dataTransfer?.getData('text/plain') || dragNodeId;
		dragOverGroup = null;
		dragNodeId = null;
		if (!id) return;
		busy = true;
		error = '';
		message = '';
		try {
			const g = await addGroupMember(groupId, id, 1);
			groups = groups.map((x) => (x.id === g.id ? { ...g, members: g.members ?? [] } : x));
		} catch (err) {
			error = err instanceof ApiClientError ? apiErrorText(err) : t('groups.saveFailed');
		} finally {
			busy = false;
		}
	}

	async function onRemoveMember(groupId: string, nodeId: string) {
		busy = true;
		error = '';
		try {
			await removeGroupMember(groupId, nodeId);
			await reload();
		} catch (err) {
			error = err instanceof ApiClientError ? apiErrorText(err) : t('groups.deleteFailed');
		} finally {
			busy = false;
		}
	}

	async function onWeight(groupId: string, nodeId: string, weight: number) {
		const w = Math.max(1, Math.min(99, Math.floor(weight) || 1));
		try {
			await setGroupMemberWeight(groupId, nodeId, w);
			groups = groups.map((g) =>
				g.id !== groupId
					? g
					: {
							...g,
							members: (g.members ?? []).map((m) =>
								m.node_id === nodeId ? { ...m, weight: w } : m
							)
						}
			);
		} catch (err) {
			error = err instanceof ApiClientError ? apiErrorText(err) : t('groups.saveFailed');
		}
	}

	async function onCreateGroup() {
		const name = newGroupName.trim();
		if (!name) {
			error = t('groups.nameRequired');
			return;
		}
		busy = true;
		error = '';
		try {
			const g = await createGroup({ name, policy: newPolicy || 'fixed' });
			groups = [...groups, { ...g, members: g.members ?? [] }];
			newGroupName = '';
		} catch (err) {
			error = err instanceof ApiClientError ? apiErrorText(err) : t('groups.saveFailed');
		} finally {
			busy = false;
		}
	}

	function moveRule(i: number, dir: -1 | 1) {
		const j = i + dir;
		if (j < 0 || j >= rules.length) return;
		const next = [...rules];
		const tmp = next[i];
		next[i] = next[j];
		next[j] = tmp;
		rules = next;
	}

	function addRule() {
		const out = groups[0]?.name || 'proxy';
		rules = [...rules, { expression: 'domain(example.com)', outbound: out, enabled: true }];
	}

	function removeRule(i: number) {
		rules = rules.filter((_, idx) => idx !== i);
	}

	async function saveRouting() {
		busy = true;
		error = '';
		message = '';
		try {
			const doc = await putRouting({
				rules: rules.map((r) => ({
					expression: r.expression.trim(),
					outbound: r.outbound.trim(),
					enabled: r.enabled
				})),
				fallback: fallback.trim() || 'proxy'
			});
			rules = doc.rules;
			fallback = doc.fallback;
			message = t('flow.saved');
		} catch (err) {
			error = err instanceof ApiClientError ? apiErrorText(err) : t('routing.saveFailed');
		} finally {
			busy = false;
		}
	}

	const outbounds = $derived([
		...groups.map((g) => g.name),
		'direct',
		'must_direct',
		'block',
		'proxy'
	]);
</script>

<span class="eyebrow">orchestrate · visual flow</span>
<h1 class="page-title">{t('flow.title')}</h1>
<p class="page-sub">{t('flow.subtitle')}</p>

{#if error}
	<p class="error" role="alert">{error}</p>
{/if}
{#if message}
	<p class="ok" role="status">{message}</p>
{/if}

<div class="flow-grid">
	<!-- Node pool -->
	<section class="panel lane">
		<header class="lane-head">
			<span class="step">01</span>
			<div>
				<h2>{t('flow.pool')}</h2>
				<p class="lane-hint">{t('flow.dropHint')}</p>
			</div>
		</header>
		<div class="pool">
			{#if !freeNodes.length && !nodes.length}
				<p class="muted empty">{t('flow.emptyPool')}</p>
			{:else if !freeNodes.length}
				<p class="muted empty">All nodes assigned</p>
			{:else}
				{#each freeNodes as n (n.id)}
					<div
						class="chip"
						class:dragging={dragNodeId === n.id}
						draggable="true"
						ondragstart={(e) => onDragStart(n.id, e)}
						ondragend={onDragEnd}
						role="listitem"
					>
						<span class="chip-name">{n.name}</span>
						<span class="chip-meta mono">{n.protocol ?? '—'} · {n.address ?? '—'}</span>
					</div>
				{/each}
			{/if}
		</div>
	</section>

	<!-- Groups as flow nodes -->
	<section class="panel lane">
		<header class="lane-head">
			<span class="step">02</span>
			<div>
				<h2>{t('flow.groups')}</h2>
				<p class="lane-hint">{t('flow.policy')}: fixed / random / min_moving_avg</p>
			</div>
		</header>

		<div class="group-create">
			<input placeholder={t('groups.name')} bind:value={newGroupName} disabled={busy} />
			<select bind:value={newPolicy} disabled={busy}>
				<option value="fixed">fixed (weight)</option>
				<option value="random">random (weight)</option>
				<option value="min_moving_avg">min_moving_avg</option>
				<option value="min">min</option>
			</select>
			<button type="button" class="primary" disabled={busy} onclick={onCreateGroup}
				>{t('flow.addGroup')}</button
			>
		</div>

		<div class="group-stack">
			{#each groups as g (g.id)}
				<div
					class="group-node"
					class:over={dragOverGroup === g.id}
					ondragover={(e) => {
						e.preventDefault();
						dragOverGroup = g.id;
					}}
					ondragleave={() => {
						if (dragOverGroup === g.id) dragOverGroup = null;
					}}
					ondrop={(e) => dropOnGroup(g.id, e)}
					role="region"
					aria-label={g.name}
				>
					<div class="group-head">
						<strong class="gname">{g.name}</strong>
						<span class="gpill mono">{g.policy}</span>
					</div>
					{#if !(g.members?.length)}
						<p class="muted dropzone">{t('flow.emptyGroup')}</p>
					{:else}
						<ul class="member-list">
							{#each g.members as m (m.node_id)}
								<li>
									<div class="m-main">
										<span class="m-name">{m.name ?? m.node_id.slice(0, 8)}</span>
										<span class="m-meta mono">{m.protocol ?? ''} {m.address ?? ''}</span>
									</div>
									<label class="wlab">
										<span class="sr">{t('flow.weight')}</span>
										<input
											type="number"
											min="1"
											max="99"
											value={m.weight}
											disabled={busy}
											onchange={(e) =>
												onWeight(
													g.id,
													m.node_id,
													Number((e.currentTarget as HTMLInputElement).value)
												)}
										/>
									</label>
									<button
										type="button"
										class="danger sm"
										disabled={busy}
										onclick={() => onRemoveMember(g.id, m.node_id)}>×</button
									>
								</li>
							{/each}
						</ul>
					{/if}
					<div class="flow-arrow" aria-hidden="true">↓</div>
				</div>
			{/each}
		</div>
	</section>

	<!-- Routing chain -->
	<section class="panel lane">
		<header class="lane-head">
			<span class="step">03</span>
			<div>
				<h2>{t('flow.rules')}</h2>
				<p class="lane-hint">match → outbound (order matters)</p>
			</div>
		</header>

		<label class="fallback-lab">
			{t('flow.fallback')}
			<input bind:value={fallback} disabled={busy} list="outbound-list" />
		</label>
		<datalist id="outbound-list">
			{#each outbounds as o}
				<option value={o}></option>
			{/each}
		</datalist>

		<ol class="rule-chain">
			{#each rules as r, i (i)}
				<li class="rule-card" class:off={!r.enabled}>
					<div class="rule-ord mono">{String(i + 1).padStart(2, '0')}</div>
					<div class="rule-body">
						<label>
							{t('flow.match')}
							<input class="wide" bind:value={r.expression} disabled={busy} />
						</label>
						<label>
							{t('flow.outbound')}
							<input bind:value={r.outbound} disabled={busy} list="outbound-list" />
						</label>
						<label class="chk">
							<input type="checkbox" bind:checked={r.enabled} disabled={busy} />
							{t('common.enabled')}
						</label>
					</div>
					<div class="rule-ops">
						<button type="button" class="ghost sm" disabled={busy || i === 0} onclick={() => moveRule(i, -1)}
							>{t('flow.moveUp')}</button
						>
						<button
							type="button"
							class="ghost sm"
							disabled={busy || i === rules.length - 1}
							onclick={() => moveRule(i, 1)}>{t('flow.moveDown')}</button
						>
						<button type="button" class="danger sm" disabled={busy} onclick={() => removeRule(i)}
							>×</button
						>
					</div>
				</li>
			{/each}
		</ol>

		<div class="actions">
			<button type="button" disabled={busy} onclick={addRule}>{t('flow.addRule')}</button>
			<button type="button" class="primary" disabled={busy} onclick={saveRouting}
				>{busy ? t('common.saving') : t('flow.saveRules')}</button
			>
		</div>
	</section>
</div>

<style>
	.flow-grid {
		display: grid;
		grid-template-columns: minmax(14rem, 1fr) minmax(16rem, 1.2fr) minmax(18rem, 1.3fr);
		gap: var(--space-sm);
		align-items: start;
	}
	.lane {
		padding: var(--space-md);
		min-height: 20rem;
	}
	.lane-head {
		display: flex;
		gap: 0.75rem;
		align-items: flex-start;
		margin-bottom: var(--space-md);
	}
	.lane-head h2 {
		margin: 0;
		font-size: 0.95rem;
		font-family: var(--font-mono);
	}
	.lane-hint {
		margin: 0.15rem 0 0;
		font-size: 0.75rem;
		color: var(--ink-dim);
	}
	.step {
		font-family: var(--font-mono);
		font-size: 0.75rem;
		font-weight: 700;
		color: var(--signal);
		background: var(--signal-soft);
		border: 1px solid rgba(34, 197, 94, 0.3);
		border-radius: 6px;
		padding: 0.25rem 0.4rem;
	}
	.pool {
		display: flex;
		flex-direction: column;
		gap: 0.45rem;
		max-height: 28rem;
		overflow: auto;
	}
	.chip {
		display: flex;
		flex-direction: column;
		gap: 0.15rem;
		padding: 0.55rem 0.65rem;
		border: 1px solid var(--line);
		border-radius: var(--r-md);
		background: var(--bg-raised);
		cursor: grab;
		user-select: none;
		transition: border-color 0.15s ease, box-shadow 0.15s ease, transform 0.15s ease;
	}
	.chip:hover {
		border-color: var(--signal);
		box-shadow: 0 0 0 1px rgba(34, 197, 94, 0.25);
	}
	.chip.dragging {
		opacity: 0.55;
		transform: scale(0.98);
	}
	.chip-name {
		font-weight: 600;
		font-size: 0.9rem;
	}
	.chip-meta,
	.m-meta,
	.mono {
		font-family: var(--font-mono);
		font-size: 0.72rem;
		color: var(--ink-dim);
	}
	.group-create {
		display: grid;
		grid-template-columns: 1fr 1fr auto;
		gap: 0.4rem;
		margin-bottom: var(--space-md);
	}
	.group-stack {
		display: flex;
		flex-direction: column;
		gap: 0.75rem;
	}
	.group-node {
		position: relative;
		border: 1px dashed var(--line-strong);
		border-radius: var(--r-lg);
		padding: 0.75rem;
		background: rgba(15, 23, 42, 0.65);
		transition: border-color 0.15s ease, background 0.15s ease, box-shadow 0.15s ease;
		min-height: 5.5rem;
	}
	.group-node.over {
		border-color: var(--signal);
		background: var(--signal-soft);
		box-shadow: 0 0 0 2px rgba(34, 197, 94, 0.25);
	}
	.group-head {
		display: flex;
		justify-content: space-between;
		align-items: center;
		gap: 0.5rem;
		margin-bottom: 0.5rem;
	}
	.gname {
		font-family: var(--font-mono);
		font-size: 0.95rem;
	}
	.gpill {
		font-size: 0.68rem;
		padding: 0.15rem 0.4rem;
		border-radius: 999px;
		background: var(--bg-raised);
		border: 1px solid var(--line);
		color: var(--ink-muted);
	}
	.dropzone {
		font-size: 0.82rem;
		padding: 0.75rem;
		text-align: center;
		border: 1px dashed var(--line);
		border-radius: var(--r-md);
	}
	.member-list {
		list-style: none;
		margin: 0;
		padding: 0;
		display: flex;
		flex-direction: column;
		gap: 0.35rem;
	}
	.member-list li {
		display: grid;
		grid-template-columns: 1fr auto auto;
		gap: 0.4rem;
		align-items: center;
		padding: 0.4rem 0.5rem;
		background: var(--bg-raised);
		border: 1px solid var(--line);
		border-radius: var(--r-md);
	}
	.m-name {
		font-weight: 600;
		font-size: 0.85rem;
	}
	.wlab input {
		width: 3.2rem;
		padding: 0.3rem 0.35rem;
		font-family: var(--font-mono);
		font-size: 0.8rem;
	}
	.sm {
		padding: 0.25rem 0.45rem;
		font-size: 0.78rem;
	}
	.flow-arrow {
		position: absolute;
		left: 50%;
		bottom: -0.85rem;
		transform: translateX(-50%);
		color: var(--signal);
		font-size: 0.85rem;
		opacity: 0.7;
		pointer-events: none;
	}
	.group-stack .group-node:last-child .flow-arrow {
		display: none;
	}
	.fallback-lab {
		display: flex;
		flex-direction: column;
		gap: 0.3rem;
		font-size: 0.7rem;
		font-family: var(--font-mono);
		text-transform: uppercase;
		letter-spacing: 0.05em;
		color: var(--ink-dim);
		margin-bottom: var(--space-md);
	}
	.rule-chain {
		list-style: none;
		margin: 0;
		padding: 0;
		display: flex;
		flex-direction: column;
		gap: 0.5rem;
		max-height: 28rem;
		overflow: auto;
	}
	.rule-card {
		display: grid;
		grid-template-columns: auto 1fr auto;
		gap: 0.55rem;
		padding: 0.65rem;
		border: 1px solid var(--line);
		border-radius: var(--r-md);
		background: var(--bg-raised);
	}
	.rule-card.off {
		opacity: 0.55;
	}
	.rule-ord {
		font-weight: 700;
		color: var(--signal);
		padding-top: 0.35rem;
	}
	.rule-body {
		display: flex;
		flex-direction: column;
		gap: 0.35rem;
	}
	.rule-body label {
		display: flex;
		flex-direction: column;
		gap: 0.2rem;
		font-size: 0.68rem;
		text-transform: uppercase;
		letter-spacing: 0.04em;
		color: var(--ink-dim);
		font-family: var(--font-mono);
	}
	.rule-body input.wide {
		width: 100%;
		font-family: var(--font-mono);
		font-size: 0.8rem;
	}
	.chk {
		flex-direction: row !important;
		align-items: center;
		gap: 0.4rem !important;
		text-transform: none !important;
		font-size: 0.8rem !important;
		color: var(--ink-muted) !important;
	}
	.rule-ops {
		display: flex;
		flex-direction: column;
		gap: 0.25rem;
	}
	.empty {
		padding: 1rem;
		text-align: center;
	}
	.sr {
		position: absolute;
		width: 1px;
		height: 1px;
		overflow: hidden;
		clip: rect(0, 0, 0, 0);
	}

	@media (max-width: 1100px) {
		.flow-grid {
			grid-template-columns: 1fr;
		}
	}
</style>
