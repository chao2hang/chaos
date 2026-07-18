<script lang="ts">
	import { onMount } from 'svelte';
	import { getRouting, putRouting, ApiClientError, type RoutingRuleDto } from '$lib/api';
	import { apiErrorText, t } from '$lib/i18n.svelte';

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

<span class="eyebrow">policy · match rules</span>
<h1 class="page-title">{t('routing.title')}</h1>
<p class="page-sub">{t('routing.subtitle')}</p>
<p class="page-hint">{t('routing.hint')}</p>

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
	input.wide {
		width: 100%;
		min-width: 14rem;
		font-family: var(--font-mono);
		font-size: 0.85rem;
	}
	input[type='checkbox'] {
		width: 1rem;
		height: 1rem;
		accent-color: var(--signal);
	}
</style>
