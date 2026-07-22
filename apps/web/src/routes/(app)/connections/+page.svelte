<script lang="ts">
	import { onMount, onDestroy } from 'svelte';
	import { Activity, RefreshCw } from '@lucide/svelte';
	import { api } from '$lib/api';
	import { t } from '$lib/i18n.svelte';
	import AppPage from '$lib/components/ui/AppPage.svelte';
	import Button from '$lib/components/ui/Button.svelte';
	import EmptyState from '$lib/components/ui/EmptyState.svelte';
	import LoadingState from '$lib/components/ui/LoadingState.svelte';
	import PageHeader from '$lib/components/ui/PageHeader.svelte';
	import Section from '$lib/components/ui/Section.svelte';
	import { toast } from '$lib/toast.svelte';

	type Connection = {
		id: string;
		source: string;
		destination: string;
		outbound: string;
		protocol: string;
		started_at: string;
		duration_secs: number;
		bytes_up: number;
		bytes_down: number;
	};

	let connections = $state<Connection[]>([]);
	let loading = $state(true);
	let autoRefresh = $state(true);
	let interval: ReturnType<typeof setInterval> | null = null;

	async function load() {
		try {
			const res = await api<{ connections: Connection[] }>('/api/v1/runtime/connections');
			connections = res.connections;
		} catch {
			toast.error({ title: t('connections.loadFailed') });
		} finally {
			loading = false;
		}
	}

	function formatBytes(bytes: number): string {
		if (bytes === 0) return '0 B';
		if (bytes < 1024) return `${bytes} B`;
		if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
		return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
	}

	function formatDuration(secs: number): string {
		if (secs < 60) return `${secs}s`;
		if (secs < 3600) return `${Math.floor(secs / 60)}m ${secs % 60}s`;
		return `${Math.floor(secs / 3600)}h ${Math.floor((secs % 3600) / 60)}m`;
	}

	onMount(() => {
		void load();
		interval = setInterval(() => {
			if (autoRefresh) void load();
		}, 5000);
	});

	onDestroy(() => {
		if (interval) clearInterval(interval);
	});
</script>

<AppPage>
	<PageHeader title={t('connections.title')} description={t('connections.subtitle')} meta="chaos / connections">
		{#snippet actions()}
			<Button
				variant="ghost"
				size="icon"
				icon={RefreshCw}
				aria-label={t('common.refresh')}
				title={t('common.refresh')}
				onclick={() => load()}
			/>
		{/snippet}
	</PageHeader>

	{#if loading}
		<LoadingState label={t('connections.loading')} />
	{:else}
		<Section title={t('connections.active')} count={connections.length} flush>
			{#if connections.length === 0}
				<EmptyState
					icon={Activity}
					title={t('connections.empty')}
					description={t('connections.emptyDescription')}
				/>
			{:else}
				<table>
					<thead>
						<tr>
							<th>{t('connections.col.source')}</th>
							<th>{t('connections.col.destination')}</th>
							<th>{t('connections.col.outbound')}</th>
							<th>{t('connections.col.protocol')}</th>
							<th>{t('connections.col.traffic')}</th>
						</tr>
					</thead>
					<tbody>
						{#each connections as conn (conn.id)}
							<tr>
								<td><code>{conn.source}</code></td>
								<td><code>{conn.destination}</code></td>
								<td>{conn.outbound || '—'}</td>
								<td>{conn.protocol}</td>
								<td class="mono">{formatBytes(conn.bytes_up)} ↑ {formatBytes(conn.bytes_down)} ↓</td>
							</tr>
						{/each}
					</tbody>
				</table>
			{/if}
		</Section>
	{/if}
</AppPage>
