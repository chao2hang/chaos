<script lang="ts">
	import { onMount } from 'svelte';
	import {
		listNodes,
		importNodes,
		deleteNode,
		listLatency,
		testLatency,
		ApiClientError,
		type NodeDto,
		type LatencyDto
	} from '$lib/api';
	import { latencyTone, formatLatencyMs, latencyClass } from '$lib/latency';

	let nodes = $state<NodeDto[]>([]);
	let latencyById = $state<Record<string, LatencyDto>>({});
	let importText = $state('');
	let error = $state('');
	let message = $state('');
	let busy = $state(false);
	let testingId = $state<string | null>(null);

	function mergeLatency(results: LatencyDto[]) {
		const next = { ...latencyById };
		for (const r of results) next[r.id] = r;
		latencyById = next;
	}

	async function load() {
		error = '';
		try {
			const [n, lat] = await Promise.all([listNodes(), listLatency()]);
			nodes = n.nodes;
			const map: Record<string, LatencyDto> = {};
			for (const r of lat.results) map[r.id] = r;
			latencyById = map;
		} catch (e) {
			error = e instanceof ApiClientError ? e.message : 'Failed to load nodes';
		}
	}

	onMount(() => {
		void load();
	});

	async function onImport() {
		error = '';
		message = '';
		const lines = importText
			.split(/\r?\n/)
			.map((l) => l.trim())
			.filter(Boolean);
		if (!lines.length) {
			error = 'Paste one share link per line.';
			return;
		}
		busy = true;
		try {
			const res = await importNodes(lines.map((link) => ({ link })));
			const ok = res.results.filter((r) => r.ok).length;
			const fail = res.results.length - ok;
			message = `Imported ${ok} node(s)${fail ? `, ${fail} failed` : ''}`;
			if (fail) {
				const errs = res.results
					.filter((r): r is Extract<typeof r, { ok: false }> => !r.ok)
					.map((r) => r.error.message)
					.slice(0, 3);
				if (errs.length) error = errs.join('; ');
			}
			importText = '';
			await load();
		} catch (e) {
			error = e instanceof ApiClientError ? e.message : 'Import failed';
		} finally {
			busy = false;
		}
	}

	async function onTestOne(id: string) {
		testingId = id;
		error = '';
		try {
			const res = await testLatency([id]);
			mergeLatency(res.results);
		} catch (e) {
			error = e instanceof ApiClientError ? e.message : 'Latency test failed';
		} finally {
			testingId = null;
		}
	}

	async function onDelete(id: string) {
		if (!confirm('Delete this node?')) return;
		error = '';
		try {
			await deleteNode(id);
			nodes = nodes.filter((n) => n.id !== id);
			const { [id]: _, ...rest } = latencyById;
			latencyById = rest;
		} catch (e) {
			error = e instanceof ApiClientError ? e.message : 'Delete failed';
		}
	}
</script>

<h1>Nodes</h1>
<p class="muted">Import share links and run TCP latency probes.</p>

{#if error}
	<p class="error" role="alert">{error}</p>
{/if}
{#if message}
	<p class="ok" role="status">{message}</p>
{/if}

<section class="import">
	<label for="links">Share links (one per line)</label>
	<textarea
		id="links"
		rows="5"
		placeholder="trojan://…&#10;ss://…&#10;vmess://…"
		bind:value={importText}
		disabled={busy}
	></textarea>
	<button type="button" class="primary" disabled={busy} onclick={onImport}>
		{busy ? 'Importing…' : 'Import'}
	</button>
</section>

<section class="table-wrap">
	<table>
		<thead>
			<tr>
				<th>Name</th>
				<th>Protocol</th>
				<th>Address</th>
				<th>Latency</th>
				<th></th>
			</tr>
		</thead>
		<tbody>
			{#if !nodes.length}
				<tr>
					<td colspan="5" class="muted">No nodes yet.</td>
				</tr>
			{:else}
				{#each nodes as n (n.id)}
					{@const lat = latencyById[n.id]}
					{@const tone = lat
						? latencyTone(lat.latency_ms, lat.alive)
						: 'unknown'}
					<tr>
						<td>
							<div class="name">{n.name}</div>
							{#if n.tag}<span class="tag">{n.tag}</span>{/if}
						</td>
						<td>{n.protocol ?? '—'}</td>
						<td class="addr">{n.address ?? '—'}</td>
						<td class={latencyClass(tone)}>
							{lat ? formatLatencyMs(lat.latency_ms, lat.alive) : '—'}
							{#if lat?.message && !lat.alive}
								<span class="msg" title={lat.message}>!</span>
							{/if}
						</td>
						<td class="row-actions">
							<button
								type="button"
								disabled={testingId === n.id}
								onclick={() => onTestOne(n.id)}
							>
								{testingId === n.id ? '…' : 'Test'}
							</button>
							<button type="button" class="danger" onclick={() => onDelete(n.id)}
								>Delete</button
							>
						</td>
					</tr>
				{/each}
			{/if}
		</tbody>
	</table>
</section>

<style>
	h1 {
		margin: 0 0 0.25rem;
		font-size: 1.5rem;
	}
	.muted {
		color: #555;
		font-size: 0.95rem;
	}
	.error {
		color: #b42318;
		font-size: 0.9rem;
	}
	.ok {
		color: #027a48;
		font-size: 0.9rem;
	}
	.import {
		display: flex;
		flex-direction: column;
		gap: 0.5rem;
		margin: 1.25rem 0;
		background: #fff;
		border: 1px solid #e5e7eb;
		border-radius: 8px;
		padding: 1rem;
	}
	label {
		font-size: 0.9rem;
		font-weight: 600;
	}
	textarea {
		font: inherit;
		font-family: ui-monospace, monospace;
		font-size: 0.85rem;
		padding: 0.5rem 0.6rem;
		border: 1px solid #ccc;
		border-radius: 6px;
		resize: vertical;
	}
	button {
		font: inherit;
		padding: 0.45rem 0.75rem;
		border: 1px solid #ccc;
		border-radius: 6px;
		background: #fff;
		cursor: pointer;
		align-self: flex-start;
	}
	button.primary {
		background: #1a56db;
		border-color: #1a56db;
		color: #fff;
	}
	button.danger {
		color: #b42318;
		border-color: #f3b0a8;
	}
	button:disabled {
		opacity: 0.7;
		cursor: not-allowed;
	}
	.table-wrap {
		overflow-x: auto;
		background: #fff;
		border: 1px solid #e5e7eb;
		border-radius: 8px;
	}
	table {
		width: 100%;
		border-collapse: collapse;
		font-size: 0.9rem;
	}
	th,
	td {
		text-align: left;
		padding: 0.55rem 0.75rem;
		border-bottom: 1px solid #eee;
		vertical-align: top;
	}
	th {
		font-size: 0.8rem;
		text-transform: uppercase;
		letter-spacing: 0.03em;
		color: #555;
		background: #fafafa;
	}
	.name {
		font-weight: 600;
	}
	.tag {
		font-size: 0.75rem;
		color: #667085;
	}
	.addr {
		font-family: ui-monospace, monospace;
		font-size: 0.85rem;
	}
	.row-actions {
		white-space: nowrap;
		display: flex;
		gap: 0.35rem;
	}
	.msg {
		margin-left: 0.25rem;
		cursor: help;
	}
	:global(.lat-good) {
		color: #027a48;
		font-weight: 600;
	}
	:global(.lat-warn) {
		color: #b54708;
		font-weight: 600;
	}
	:global(.lat-bad) {
		color: #b42318;
		font-weight: 600;
	}
	:global(.lat-unknown) {
		color: #667085;
	}
</style>
