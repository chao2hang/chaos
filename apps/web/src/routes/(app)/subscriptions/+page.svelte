<script lang="ts">
	import { onMount } from 'svelte';
	import { Plus, RadioTower, RefreshCw, Trash2, X } from '@lucide/svelte';
	import {
		listSubscriptions,
		importSubscription,
		refreshSubscription,
		deleteSubscription,
		setSubscriptionRefresh,
		ApiClientError,
		type SubscriptionDto
	} from '$lib/api';
	import { apiErrorText, i18n, t } from '$lib/i18n.svelte';
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
	import Status from '$lib/components/ui/Status.svelte';
	import TableFrame from '$lib/components/ui/TableFrame.svelte';

	let subscriptions = $state<SubscriptionDto[]>([]);
	let url = $state('');
	let tag = $state('');
	let query = $state('');
	let error = $state('');
	let message = $state('');
	let loaded = $state(false);
	let importOpen = $state(false);
	let importing = $state(false);
	let refreshingId = $state<string | null>(null);
	let deleteTarget = $state<SubscriptionDto | null>(null);
	let deleting = $state(false);

	async function load() {
		error = '';
		try {
			const response = await listSubscriptions();
			subscriptions = response.subscriptions;
		} catch (cause) {
			error = cause instanceof ApiClientError ? apiErrorText(cause) : t('subscriptions.loadFailed');
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
		const value = url.trim();
		if (!value) {
			error = t('subscriptions.urlRequired');
			return;
		}

		importing = true;
		try {
			const response = await importSubscription(value, tag.trim() || undefined);
			message = t('subscriptions.imported', { count: response.subscription.node_count });
			url = '';
			tag = '';
			importOpen = false;
			await load();
		} catch (cause) {
			error = cause instanceof ApiClientError ? apiErrorText(cause) : t('subscriptions.importFailed');
		} finally {
			importing = false;
		}
	}

	async function onRefresh(id: string) {
		refreshingId = id;
		error = '';
		message = '';
		try {
			const response = await refreshSubscription(id);
			message = t('subscriptions.refreshed', { count: response.subscription.node_count });
			await load();
		} catch (cause) {
			error = cause instanceof ApiClientError ? apiErrorText(cause) : t('subscriptions.refreshFailed');
		} finally {
			refreshingId = null;
		}
	}

	async function onSetRefresh(id: string, hours: number) {
		try {
			await setSubscriptionRefresh(id, hours);
			await load();
		} catch (cause) {
			error = cause instanceof ApiClientError ? apiErrorText(cause) : t('subscriptions.refreshFailed');
		}
	}

	async function confirmDelete() {
		if (!deleteTarget) return;
		deleting = true;
		error = '';
		try {
			const id = deleteTarget.id;
			await deleteSubscription(id);
			subscriptions = subscriptions.filter((subscription) => subscription.id !== id);
			deleteTarget = null;
			message = t('subscriptions.deleted');
		} catch (cause) {
			error = cause instanceof ApiClientError ? apiErrorText(cause) : t('subscriptions.deleteFailed');
		} finally {
			deleting = false;
		}
	}

	function formatDate(value: string) {
		const date = new Date(value);
		if (Number.isNaN(date.getTime())) return value;
		return new Intl.DateTimeFormat(i18n.locale, {
			dateStyle: 'medium',
			timeStyle: 'short'
		}).format(date);
	}

	const filteredSubscriptions = $derived.by(() => {
		const normalized = query.trim().toLowerCase();
		if (!normalized) return subscriptions;
		return subscriptions.filter((subscription) =>
			[subscription.tag, subscription.url, subscription.status]
				.filter(Boolean)
				.some((value) => String(value).toLowerCase().includes(normalized))
		);
	});

	const totalNodes = $derived(subscriptions.reduce((total, subscription) => total + subscription.node_count, 0));
	const needsRepublish = $derived(subscriptions.some((subscription) => subscription.needs_republish));
</script>

<AppPage>
	<PageHeader
		title={t('subscriptions.title')}
		description={t('subscriptions.subtitle')}
		meta="inventory / subscriptions"
	>
		{#snippet actions()}
			<Button variant="primary" icon={importOpen ? X : Plus} onclick={() => (importOpen = !importOpen)}>
				{importOpen ? t('common.close') : t('subscriptions.importAction')}
			</Button>
		{/snippet}
	</PageHeader>

	{#if error}<Notice tone="error" message={error} ondismiss={() => (error = '')} />{/if}
	{#if message}<Notice tone="success" message={message} ondismiss={() => (message = '')} />{/if}
	{#if needsRepublish}<Notice message={t('subscriptions.needsRepublish')} />{/if}

	{#if importOpen}
		<Section title={t('subscriptions.importTitle')} description={t('subscriptions.importDescription')}>
			<form class="form-stack" onsubmit={(event) => { event.preventDefault(); void onImport(); }}>
				<div class="form-grid">
					<Field label={t('subscriptions.urlLabel')} forId="subscription-url">
						<input
							id="subscription-url"
							type="url"
							placeholder="https://"
							bind:value={url}
							disabled={importing}
							required
						/>
					</Field>
					<Field label={t('subscriptions.tagLabel')} forId="subscription-tag" optional>
						<input
							id="subscription-tag"
							type="text"
							placeholder={t('subscriptions.tagPlaceholder')}
							bind:value={tag}
							disabled={importing}
						/>
					</Field>
				</div>
				<div class="form-footer">
					<span>{t('subscriptions.importHint')}</span>
					<Button type="submit" variant="primary" icon={RadioTower} loading={importing}>
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
			placeholder={t('subscriptions.searchPlaceholder')}
			meta={t('subscriptions.counts', { subs: subscriptions.length, nodes: totalNodes })}
			refreshLabel={t('common.refresh')}
			onrefresh={load}
		/>

		{#if filteredSubscriptions.length}
			<TableFrame>
				<table>
					<thead>
						<tr>
							<th>{t('subscriptions.col.tag')}</th>
							<th>{t('subscriptions.col.url')}</th>
							<th>{t('subscriptions.col.status')}</th>
							<th>{t('subscriptions.col.nodes')}</th>
							<th>{t('subscriptions.col.updated')}</th>
							<th>{t('subscriptions.col.autoRefresh')}</th>
							<th class="actions-col">{t('common.actions')}</th>
						</tr>
					</thead>
					<tbody>
						{#each filteredSubscriptions as subscription (subscription.id)}
							<tr>
								<td data-label={t('subscriptions.col.tag')}>
									<strong>{subscription.tag ?? t('subscriptions.untagged')}</strong>
								</td>
								<td data-label={t('subscriptions.col.url')}>
									<code class="url" title={subscription.url}>{subscription.url}</code>
								</td>
								<td data-label={t('subscriptions.col.status')}>
									<Status
										label={subscription.status}
										tone={subscription.status === 'ok' ? 'positive' : 'negative'}
									/>
								</td>
								<td data-label={t('subscriptions.col.nodes')} class="node-count">{subscription.node_count}</td>
								<td data-label={t('subscriptions.col.updated')} class="date">{formatDate(subscription.updated_at)}</td>
								<td data-label={t('subscriptions.col.autoRefresh')}>
									<select
										class="refresh-select"
										value={subscription.refresh_interval_hours}
										onchange={(e) => onSetRefresh(subscription.id, Number(e.currentTarget.value))}
										title={t('subscriptions.autoRefreshTitle')}
									>
										<option value={0}>{t('subscriptions.refreshOff')}</option>
										<option value={6}>6h</option>
										<option value={12}>12h</option>
										<option value={24}>24h</option>
									</select>
								</td>
								<td data-label={t('common.actions')}>
									<div class="row-actions">
										<Button
											variant="ghost"
											size="icon"
											icon={RefreshCw}
											loading={refreshingId === subscription.id}
											disabled={!!refreshingId}
											aria-label={t('subscriptions.refreshNamed', { name: subscription.tag ?? subscription.url })}
											title={t('subscriptions.refresh')}
											onclick={() => onRefresh(subscription.id)}
										/>
										<Button
											variant="ghost"
											size="icon"
											icon={Trash2}
											aria-label={t('subscriptions.deleteNamed', { name: subscription.tag ?? subscription.url })}
											title={t('common.delete')}
											onclick={() => (deleteTarget = subscription)}
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
					icon={RadioTower}
					title={query ? t('common.noSearchResults') : t('subscriptions.empty')}
					description={query ? t('common.tryDifferentSearch') : t('subscriptions.emptyDescription')}
				>
					{#snippet actions()}
						{#if !query}
							<Button icon={Plus} onclick={() => (importOpen = true)}>{t('subscriptions.importAction')}</Button>
						{/if}
					{/snippet}
				</EmptyState>
			</Section>
		{/if}
	{/if}
</AppPage>

<ConfirmDialog
	open={!!deleteTarget}
	title={t('subscriptions.deleteTitle')}
	description={t('subscriptions.deleteDescription', { name: deleteTarget?.tag ?? deleteTarget?.url ?? '' })}
	confirmLabel={t('common.delete')}
	cancelLabel={t('common.cancel')}
	busy={deleting}
	danger
	onconfirm={confirmDelete}
	oncancel={() => (deleteTarget = null)}
/>

<style>
	.form-grid {
		display: grid;
		grid-template-columns: minmax(0, 2fr) minmax(10rem, 1fr);
		gap: var(--space-4);
	}

	.url {
		display: block;
		max-width: 22rem;
		overflow: hidden;
		color: var(--ink-muted);
		font-size: 0.72rem;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.node-count {
		font-family: var(--font-mono);
		font-variant-numeric: tabular-nums;
	}

	.date {
		color: var(--ink-muted);
		font-size: 0.74rem;
		white-space: nowrap;
	}

	.refresh-select {
		width: auto;
		min-height: 1.75rem;
		padding: 0.2rem 0.4rem;
		font-size: 0.72rem;
		border: 1px solid var(--line);
		border-radius: var(--radius-sm);
		background: var(--surface);
		color: var(--ink-muted);
	}

	@media (max-width: 720px) {
		.form-grid {
			grid-template-columns: 1fr;
		}

		.url {
			justify-self: end;
		}

		.url {
			max-width: min(16rem, 55vw);
		}
	}
</style>
