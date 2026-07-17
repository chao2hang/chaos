<script lang="ts">
	import { onMount } from 'svelte';
	import {
		listSubscriptions,
		importSubscription,
		refreshSubscription,
		deleteSubscription,
		ApiClientError,
		type SubscriptionDto
	} from '$lib/api';

	let subs = $state<SubscriptionDto[]>([]);
	let url = $state('');
	let tag = $state('');
	let error = $state('');
	let message = $state('');
	let busy = $state(false);
	let refreshingId = $state<string | null>(null);

	async function load() {
		error = '';
		try {
			const res = await listSubscriptions();
			subs = res.subscriptions;
		} catch (e) {
			error = e instanceof ApiClientError ? e.message : 'Failed to load subscriptions';
		}
	}

	onMount(() => {
		void load();
	});

	async function onImport() {
		error = '';
		message = '';
		const u = url.trim();
		if (!u) {
			error = 'URL is required';
			return;
		}
		busy = true;
		try {
			const res = await importSubscription(u, tag.trim() || undefined);
			message = `Imported subscription (${res.subscription.node_count} nodes)`;
			url = '';
			tag = '';
			await load();
		} catch (e) {
			error = e instanceof ApiClientError ? e.message : 'Import failed';
			await load();
		} finally {
			busy = false;
		}
	}

	async function onRefresh(id: string) {
		refreshingId = id;
		error = '';
		message = '';
		try {
			const res = await refreshSubscription(id);
			message = `Refreshed: ${res.subscription.node_count} nodes`;
			await load();
		} catch (e) {
			error = e instanceof ApiClientError ? e.message : 'Refresh failed';
			await load();
		} finally {
			refreshingId = null;
		}
	}

	async function onDelete(id: string) {
		if (!confirm('Delete this subscription and its nodes?')) return;
		error = '';
		try {
			await deleteSubscription(id);
			subs = subs.filter((s) => s.id !== id);
			message = 'Subscription deleted';
		} catch (e) {
			error = e instanceof ApiClientError ? e.message : 'Delete failed';
		}
	}

	const totalNodes = $derived(subs.reduce((n, s) => n + s.node_count, 0));
</script>

<h1>Subscriptions</h1>
<p class="muted">Import a subscription URL; refresh re-fetches and replaces nodes.</p>

{#if error}
	<p class="error" role="alert">{error}</p>
{/if}
{#if message}
	<p class="ok" role="status">{message}</p>
{/if}

<section class="import">
	<label for="sub-url">Subscription URL</label>
	<input
		id="sub-url"
		type="url"
		placeholder="https://…"
		bind:value={url}
		disabled={busy}
	/>
	<label for="sub-tag">Tag (optional)</label>
	<input id="sub-tag" type="text" placeholder="home" bind:value={tag} disabled={busy} />
	<button type="button" class="primary" disabled={busy} onclick={onImport}>
		{busy ? 'Importing…' : 'Import'}
	</button>
</section>

<p class="counts">
	{subs.length} subscription(s) · {totalNodes} node(s) total
</p>

<section class="table-wrap">
	<table>
		<thead>
			<tr>
				<th>Tag</th>
				<th>URL</th>
				<th>Status</th>
				<th>Nodes</th>
				<th>Updated</th>
				<th></th>
			</tr>
		</thead>
		<tbody>
			{#if !subs.length}
				<tr>
					<td colspan="6" class="muted">No subscriptions yet.</td>
				</tr>
			{:else}
				{#each subs as s (s.id)}
					<tr>
						<td>{s.tag ?? '—'}</td>
						<td class="url" title={s.url}>{s.url}</td>
						<td>
							<span class:status-ok={s.status === 'ok'} class:status-err={s.status === 'error'}
								>{s.status}</span
							>
						</td>
						<td><strong>{s.node_count}</strong></td>
						<td class="ts">{s.updated_at}</td>
						<td class="row-actions">
							<button
								type="button"
								disabled={refreshingId === s.id}
								onclick={() => onRefresh(s.id)}
							>
								{refreshingId === s.id ? '…' : 'Refresh'}
							</button>
							<button type="button" class="danger" onclick={() => onDelete(s.id)}
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
	input {
		font: inherit;
		padding: 0.5rem 0.6rem;
		border: 1px solid #ccc;
		border-radius: 6px;
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
	.counts {
		font-size: 0.9rem;
		color: #555;
		margin: 0 0 0.75rem;
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
	.url {
		max-width: 14rem;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		font-family: ui-monospace, monospace;
		font-size: 0.8rem;
	}
	.ts {
		font-size: 0.8rem;
		color: #555;
		white-space: nowrap;
	}
	.row-actions {
		white-space: nowrap;
		display: flex;
		gap: 0.35rem;
	}
	.status-ok {
		color: #027a48;
		font-weight: 600;
	}
	.status-err {
		color: #b42318;
		font-weight: 600;
	}
</style>
