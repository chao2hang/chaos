<script lang="ts">
	import { onMount } from 'svelte';
	import { getRouting, putRouting, ApiClientError, type RoutingRuleDto } from '$lib/api';
	import { apiErrorText, t } from '$lib/i18n';

	let rules = $state<RoutingRuleDto[]>([]);
	let fallback = $state('proxy');
	let error = $state('');
	let message = $state('');
	let busy = $state(false);

	async function load() {
		error = '';
		try {
			const doc = await getRouting();
			rules = doc.rules;
			fallback = doc.fallback;
		} catch (e) {
			error = e instanceof ApiClientError ? apiErrorText(e) : t('routing.loadFailed');
		}
	}

	onMount(() => {
		void load();
	});

	function addRule() {
		rules = [...rules, { expression: '', outbound: 'proxy', enabled: true }];
	}

	function removeRule(i: number) {
		rules = rules.filter((_, idx) => idx !== i);
	}

	async function onSave() {
		error = '';
		message = '';
		if (!fallback.trim()) {
			error = t('routing.fallbackRequired');
			return;
		}
		busy = true;
		try {
			const doc = await putRouting({
				rules: rules.map((r) => ({
					expression: r.expression.trim(),
					outbound: r.outbound.trim(),
					enabled: r.enabled
				})),
				fallback: fallback.trim()
			});
			rules = doc.rules;
			fallback = doc.fallback;
			message = t('routing.saved');
		} catch (e) {
			error = e instanceof ApiClientError ? apiErrorText(e) : t('routing.saveFailed');
		} finally {
			busy = false;
		}
	}
</script>

<h1>{t('routing.title')}</h1>
<p class="muted">{t('routing.subtitle')}</p>
<p class="hint">{t('routing.hint')}</p>

{#if error}
	<p class="error" role="alert">{error}</p>
{/if}
{#if message}
	<p class="ok" role="status">{message}</p>
{/if}

<section class="import">
	<label for="fallback">{t('routing.fallback')}</label>
	<input id="fallback" bind:value={fallback} disabled={busy} />
</section>

<section class="table-wrap">
	<table>
		<thead>
			<tr>
				<th>{t('common.enabled')}</th>
				<th>{t('routing.expression')}</th>
				<th>{t('routing.outbound')}</th>
				<th></th>
			</tr>
		</thead>
		<tbody>
			{#each rules as r, i (i)}
				<tr>
					<td>
						<input type="checkbox" bind:checked={r.enabled} disabled={busy} />
					</td>
					<td>
						<input class="wide" bind:value={r.expression} disabled={busy} />
					</td>
					<td>
						<input bind:value={r.outbound} disabled={busy} />
					</td>
					<td>
						<button type="button" class="danger" disabled={busy} onclick={() => removeRule(i)}
							>{t('common.delete')}</button
						>
					</td>
				</tr>
			{/each}
		</tbody>
	</table>
</section>

<div class="actions">
	<button type="button" disabled={busy} onclick={addRule}>{t('routing.addRule')}</button>
	<button type="button" class="primary" disabled={busy} onclick={onSave}>
		{busy ? t('common.saving') : t('common.save')}
	</button>
</div>

<style>
	h1 {
		margin: 0 0 0.25rem;
		font-size: 1.5rem;
	}
	.muted {
		color: #555;
		font-size: 0.95rem;
	}
	.hint {
		color: #667085;
		font-size: 0.85rem;
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
		margin: 1rem 0;
		display: flex;
		flex-direction: column;
		gap: 0.4rem;
		max-width: 24rem;
	}
	label {
		font-size: 0.9rem;
		font-weight: 600;
	}
	input {
		font: inherit;
		padding: 0.45rem 0.55rem;
		border: 1px solid #ccc;
		border-radius: 6px;
	}
	input.wide {
		width: 100%;
		min-width: 14rem;
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
		padding: 0.5rem 0.65rem;
		border-bottom: 1px solid #eee;
		vertical-align: middle;
	}
	th {
		font-size: 0.8rem;
		text-transform: uppercase;
		color: #555;
		background: #fafafa;
	}
	.actions {
		display: flex;
		gap: 0.5rem;
		margin-top: 1rem;
	}
	button {
		font: inherit;
		padding: 0.5rem 0.8rem;
		border: 1px solid #ccc;
		border-radius: 6px;
		background: #fff;
		cursor: pointer;
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
</style>
