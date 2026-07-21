<script lang="ts">
	import { onMount } from 'svelte';
	import { Gauge, Import, Plus, Server, Trash2, X } from '@lucide/svelte';
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
	import { sortByLatency } from '$lib/latencySessionCore';
	import { apiErrorText, t } from '$lib/i18n.svelte';
	import { countryFlag } from '$lib/utils';
	import Button from '$lib/components/ui/Button.svelte';
	import AppPage from '$lib/components/ui/AppPage.svelte';
	import ConfirmDialog from '$lib/components/ui/ConfirmDialog.svelte';
	import EmptyState from '$lib/components/ui/EmptyState.svelte';
	import Field from '$lib/components/ui/Field.svelte';
	import LoadingState from '$lib/components/ui/LoadingState.svelte';
	import Notice from '$lib/components/ui/Notice.svelte';
	import PageHeader from '$lib/components/ui/PageHeader.svelte';
	import ResourceToolbar from '$lib/components/ui/ResourceToolbar.svelte';
	import Section from '$lib/components/ui/Section.svelte';
	import TableFrame from '$lib/components/ui/TableFrame.svelte';

	let nodes = $state<NodeDto[]>([]);
	let latencyById = $state<Record<string, LatencyDto>>({});
	let importText = $state('');
	let query = $state('');
	let selectedIds = $state<string[]>([]);
	let error = $state('');
	let message = $state('');
	let loaded = $state(false);
	let importOpen = $state(false);
	let importing = $state(false);
	let testing = $state<string | null>(null);
	let deleting = $state(false);
	let deleteTarget = $state<NodeDto | null>(null);

	function mergeLatency(results: LatencyDto[]) {
		const next = { ...latencyById };
		for (const result of results) next[result.id] = result;
		latencyById = next;
	}

	async function load() {
		error = '';
		try {
			const [nodeResult, latencyResult] = await Promise.all([listNodes(), listLatency()]);
			nodes = nodeResult.nodes;
			const map: Record<string, LatencyDto> = {};
			for (const result of latencyResult.results) map[result.id] = result;
			latencyById = map;
			selectedIds = selectedIds.filter((id) => nodes.some((node) => node.id === id));
		} catch (cause) {
			error = cause instanceof ApiClientError ? apiErrorText(cause) : t('nodes.loadFailed');
		} finally {
			loaded = true;
		}
	}

	onMount(() => {
		void load();
	});

	async function onImport() {
		error = '';
		message = '';
		const links = importText
			.split(/\r?\n/)
			.map((line) => line.trim())
			.filter(Boolean);
		if (!links.length) {
			error = t('nodes.importEmpty');
			return;
		}

		importing = true;
		try {
			const response = await importNodes(links.map((link) => ({ link })));
			const successful = response.results.filter((result) => result.ok).length;
			const failedResults = response.results.filter(
				(result): result is Extract<typeof result, { ok: false }> => !result.ok
			);
			const failSuffix = failedResults.length
				? t('nodes.importedFailSuffix', { fail: failedResults.length })
				: '';
			message = t('nodes.imported', { ok: successful, failSuffix });
			importText = failedResults.map((result) => result.link).join('\n');
			if (failedResults.length) {
				error = failedResults
					.slice(0, 3)
					.map((result) => apiErrorText(result.error))
					.join('; ');
			} else {
				importOpen = false;
			}
			await load();
		} catch (cause) {
			error = cause instanceof ApiClientError ? apiErrorText(cause) : t('nodes.importFailed');
		} finally {
			importing = false;
		}
	}

	async function runLatency(ids: string[] | null, marker: string) {
		testing = marker;
		error = '';
		message = '';
		try {
			const response = await testLatency(ids);
			mergeLatency(response.results);
			const alive = response.results.filter((result) => result.alive).length;
			message = t('dashboard.latencyFinished', { alive, total: response.results.length });
		} catch (cause) {
			error = cause instanceof ApiClientError ? apiErrorText(cause) : t('nodes.latencyFailed');
		} finally {
			testing = null;
		}
	}

	function toggleSelected(id: string) {
		selectedIds = selectedIds.includes(id)
			? selectedIds.filter((selected) => selected !== id)
			: [...selectedIds, id];
	}

	function toggleAllVisible() {
		const visibleIds = filteredNodes.map((node) => node.id);
		const everyVisibleSelected = visibleIds.length > 0 && visibleIds.every((id) => selectedIds.includes(id));
		selectedIds = everyVisibleSelected
			? selectedIds.filter((id) => !visibleIds.includes(id))
			: Array.from(new Set([...selectedIds, ...visibleIds]));
	}

	async function confirmDelete() {
		if (!deleteTarget) return;
		deleting = true;
		error = '';
		try {
			const id = deleteTarget.id;
			await deleteNode(id);
			nodes = nodes.filter((node) => node.id !== id);
			selectedIds = selectedIds.filter((selected) => selected !== id);
			const { [id]: _removed, ...rest } = latencyById;
			latencyById = rest;
			deleteTarget = null;
			message = t('nodes.deleted');
		} catch (cause) {
			error = cause instanceof ApiClientError ? apiErrorText(cause) : t('nodes.deleteFailed');
		} finally {
			deleting = false;
		}
	}

	const filteredNodes = $derived.by(() => {
		const normalized = query.trim().toLowerCase();
		const latencyMap = latencyById;
		const matchingNodes = normalized
			? nodes.filter((node) =>
					[node.name, node.tag, node.protocol, node.address]
						.filter(Boolean)
						.some((value) => String(value).toLowerCase().includes(normalized))
				)
			: nodes;
		return sortByLatency(matchingNodes, (node) => latencyMap[node.id]);
	});

	const allVisibleSelected = $derived(
		filteredNodes.length > 0 && filteredNodes.every((node) => selectedIds.includes(node.id))
	);
</script>

<AppPage>
	<PageHeader title={t('nodes.title')} description={t('nodes.subtitle')} meta="inventory / nodes">
		{#snippet actions()}
			<Button icon={Gauge} disabled={!!testing || !nodes.length} onclick={() => runLatency(selectedIds.length ? selectedIds : null, 'bulk')}>
				{selectedIds.length
					? t('nodes.testSelected', { count: selectedIds.length })
					: t('dashboard.testAll')}
			</Button>
			<Button variant="primary" icon={importOpen ? X : Plus} onclick={() => (importOpen = !importOpen)}>
				{importOpen ? t('common.close') : t('nodes.importAction')}
			</Button>
		{/snippet}
	</PageHeader>

	{#if error}<Notice tone="error" message={error} ondismiss={() => (error = '')} />{/if}
	{#if message}<Notice tone="success" message={message} ondismiss={() => (message = '')} />{/if}

	{#if importOpen}
		<Section title={t('nodes.importTitle')} description={t('nodes.importDescription')}>
			<form class="form-stack" onsubmit={(event) => { event.preventDefault(); void onImport(); }}>
				<Field label={t('nodes.linksLabel')} forId="links" hint={t('nodes.importHint')}>
					<textarea
						id="links"
						rows="6"
						placeholder={t('nodes.linksPlaceholder')}
						bind:value={importText}
						disabled={importing}
					></textarea>
				</Field>
				<div class="form-footer">
					<span>{t('nodes.detectedLinks', { count: importText.split(/\r?\n/).filter((line) => line.trim()).length })}</span>
					<Button type="submit" variant="primary" icon={Import} loading={importing}>
						{importing ? t('common.importing') : t('common.import')}
					</Button>
				</div>
			</form>
		</Section>
	{/if}

	{#if !loaded}
		<LoadingState label={t('common.loading')} />
	{:else}
		<ResourceToolbar
			bind:value={query}
			placeholder={t('nodes.searchPlaceholder')}
			meta={selectedIds.length ? t('common.selectedCount', { count: selectedIds.length }) : undefined}
			refreshLabel={t('common.refresh')}
			onrefresh={load}
		/>

		{#if filteredNodes.length}
			<TableFrame>
				<table>
					<thead>
						<tr>
							<th class="select-col">
								<input
									type="checkbox"
									checked={allVisibleSelected}
									aria-label={t('common.selectAll')}
									onchange={toggleAllVisible}
								/>
							</th>
							<th>{t('nodes.col.name')}</th>
							<th>{t('nodes.col.protocol')}</th>
							<th>{t('nodes.col.address')}</th>
							<th>{t('nodes.col.latency')}</th>
							<th class="actions-col">{t('common.actions')}</th>
						</tr>
					</thead>
					<tbody>
						{#each filteredNodes as node (node.id)}
							{@const latency = latencyById[node.id]}
							{@const tone = latency ? latencyTone(latency.latency_ms, latency.alive) : 'unknown'}
							<tr class:selected={selectedIds.includes(node.id)}>
								<td class="select-col">
									<input
										type="checkbox"
										checked={selectedIds.includes(node.id)}
										aria-label={t('nodes.selectNode', { name: node.name })}
										onchange={() => toggleSelected(node.id)}
									/>
								</td>
								<td data-label={t('nodes.col.name')}>
									{#if node.country_code}<span class="flag" title={node.country_code}>{countryFlag(node.country_code)}</span>{/if}<strong>{node.name}</strong>
									{#if node.tag}<span class="tag">{node.tag}</span>{/if}
								</td>
								<td data-label={t('nodes.col.protocol')} class="data-meta">{node.protocol ?? t('common.emDash')}</td>
								<td data-label={t('nodes.col.address')} class="data-meta address">{node.address ?? t('common.emDash')}</td>
								<td data-label={t('nodes.col.latency')} class={latencyClass(tone)} title={latency?.message ?? ''}>
									{latency ? formatLatencyMs(latency.latency_ms, latency.alive) : t('common.emDash')}
								</td>
								<td data-label={t('common.actions')}>
									<div class="row-actions">
										<Button
											variant="ghost"
											size="icon"
											icon={Gauge}
											loading={testing === node.id}
											disabled={!!testing}
											aria-label={t('nodes.testNode', { name: node.name })}
											title={t('common.test')}
											onclick={() => runLatency([node.id], node.id)}
										/>
										<Button
											variant="ghost"
											size="icon"
											icon={Trash2}
											aria-label={t('nodes.deleteNode', { name: node.name })}
											title={t('common.delete')}
											onclick={() => (deleteTarget = node)}
										/>
									</div>
								</td>
							</tr>
						{/each}
					</tbody>
				</table>
			</TableFrame>
		{:else}
			<Section flush>
				<EmptyState
					icon={Server}
					title={query ? t('common.noSearchResults') : t('nodes.empty')}
					description={query ? t('common.tryDifferentSearch') : t('nodes.emptyDescription')}
				>
					{#snippet actions()}
						{#if !query}
							<Button icon={Plus} onclick={() => (importOpen = true)}>{t('nodes.importAction')}</Button>
						{/if}
					{/snippet}
				</EmptyState>
			</Section>
		{/if}
	{/if}
</AppPage>

<ConfirmDialog
	open={!!deleteTarget}
	title={t('nodes.deleteTitle')}
	description={t('nodes.deleteDescription', { name: deleteTarget?.name ?? '' })}
	confirmLabel={t('common.delete')}
	cancelLabel={t('common.cancel')}
	busy={deleting}
	danger
	onconfirm={confirmDelete}
	oncancel={() => (deleteTarget = null)}
/>

<style>
	.form-stack textarea {
		font-family: var(--font-mono);
		font-size: 0.76rem;
	}

	.select-col {
		width: 2.75rem;
		text-align: center;
	}

	tbody tr.selected {
		background: var(--surface-subtle);
	}

	td strong {
		display: block;
		font-size: 0.82rem;
	}

	.flag {
		margin-right: 0.35rem;
		font-size: 1.1em;
		vertical-align: -0.05em;
	}

	.tag {
		display: inline-block;
		margin-top: 0.18rem;
		color: var(--ink-muted);
		font-family: var(--font-mono);
		font-size: 0.66rem;
		text-decoration: underline;
		text-decoration-color: var(--line-strong);
		text-underline-offset: 0.2em;
	}

	.address {
		max-width: 18rem;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	@media (max-width: 720px) {
		td strong,
		.tag {
			justify-self: end;
		}

		.address {
			max-width: none;
		}
	}
</style>
