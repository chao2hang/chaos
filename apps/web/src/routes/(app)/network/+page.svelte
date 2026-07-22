<script lang="ts">
	import { onMount } from 'svelte';
	import { beforeNavigate } from '$app/navigation';
	import { RefreshCw, Save } from '@lucide/svelte';
	import {
		getNetwork,
		getNetworkInterfaces,
		putNetwork,
		ApiClientError,
		isSessionRedirectPending,
		type NetworkDocument,
		type NetworkInterfaceInfo
	} from '$lib/api';
	import { apiErrorText, t } from '$lib/i18n.svelte';
	import Button from '$lib/components/ui/Button.svelte';
	import AppPage from '$lib/components/ui/AppPage.svelte';
	import ConfirmDialog from '$lib/components/ui/ConfirmDialog.svelte';
	import LoadingState from '$lib/components/ui/LoadingState.svelte';
	import PageHeader from '$lib/components/ui/PageHeader.svelte';
	import Section from '$lib/components/ui/Section.svelte';
	import { toast } from '$lib/toast.svelte';

	let wanInterfaces = $state<string[]>(['auto']);
	let lanInterfaces = $state<string[]>([]);
	let autoConfigKernel = $state(true);
	let inventory = $state<NetworkInterfaceInfo[]>([]);
	let loaded = $state(false);
	let busy = $state(false);
	let savedSnapshot = $state('');
	let confirmReload = $state(false);

	function snapshot() {
		return JSON.stringify({
			wan_interfaces: wanInterfaces,
			lan_interfaces: lanInterfaces,
			auto_config_kernel_parameter: autoConfigKernel
		} satisfies NetworkDocument);
	}

	function applyDocument(doc: NetworkDocument) {
		wanInterfaces = [...doc.wan_interfaces];
		lanInterfaces = [...doc.lan_interfaces];
		autoConfigKernel = doc.auto_config_kernel_parameter;
	}

	async function load() {
		busy = true;
		try {
			const [doc, ifaces] = await Promise.all([getNetwork(), getNetworkInterfaces()]);
			applyDocument(doc);
			inventory = ifaces.interfaces;
			savedSnapshot = snapshot();
			confirmReload = false;
		} catch (cause) {
			toast.error({
				title: cause instanceof ApiClientError ? apiErrorText(cause) : t('network.loadFailed')
			});
		} finally {
			busy = false;
			loaded = true;
		}
	}

	onMount(() => {
		void load();
	});

	beforeNavigate(({ cancel }) => {
		if (!dirty || typeof window === 'undefined') return;
		if (isSessionRedirectPending()) return;
		if (sessionStorage.getItem('chaos_allow_dirty_navigation') === '1') {
			sessionStorage.removeItem('chaos_allow_dirty_navigation');
			return;
		}
		if (!window.confirm(t('common.discardDescription'))) cancel();
	});

	$effect(() => {
		if (typeof document === 'undefined') return;
		document.documentElement.dataset.chaosUnsaved = dirty ? 'true' : 'false';
	});

	function toggleWan(name: string, checked: boolean) {
		if (checked) {
			if (!wanInterfaces.includes(name)) wanInterfaces = [...wanInterfaces, name];
			return;
		}
		wanInterfaces = wanInterfaces.filter((item) => item !== name);
	}

	function toggleLan(name: string, checked: boolean) {
		if (name === 'auto') return;
		if (checked) {
			if (!lanInterfaces.includes(name)) lanInterfaces = [...lanInterfaces, name];
			return;
		}
		lanInterfaces = lanInterfaces.filter((item) => item !== name);
	}

	async function onSave() {
		if (wanInterfaces.length === 0) {
			toast.error({ title: t('error.network_wan_required') });
			return;
		}
		if (lanInterfaces.some((name) => name.toLowerCase() === 'auto')) {
			toast.error({ title: t('error.network_lan_auto_forbidden') });
			return;
		}
		busy = true;
		try {
			const saved = await putNetwork({
				wan_interfaces: wanInterfaces,
				lan_interfaces: lanInterfaces,
				auto_config_kernel_parameter: autoConfigKernel
			});
			applyDocument(saved);
			savedSnapshot = snapshot();
			toast.success({ title: t('network.saved') });
		} catch (cause) {
			toast.error({
				title: cause instanceof ApiClientError ? apiErrorText(cause) : t('network.saveFailed')
			});
		} finally {
			busy = false;
		}
	}

	function requestReload() {
		if (!dirty) {
			void load();
			return;
		}
		confirmReload = true;
	}

	async function confirmDiscard() {
		confirmReload = false;
		await load();
	}

	function onBeforeUnload(event: BeforeUnloadEvent) {
		if (!dirty || isSessionRedirectPending()) return;
		event.preventDefault();
		event.returnValue = '';
	}

	const dirty = $derived(loaded && snapshot() !== savedSnapshot);

	function ifaceMeta(iface: NetworkInterfaceInfo) {
		const parts: string[] = [];
		if (iface.ips.length) parts.push(iface.ips.join(', '));
		if (iface.is_default_route) parts.push(t('network.defaultRoute'));
		if (!iface.up) parts.push(t('network.down'));
		return parts.join(' · ');
	}
</script>

<svelte:window onbeforeunload={onBeforeUnload} />

<AppPage>
	<PageHeader title={t('network.title')} description={t('network.subtitle')} meta="network / interfaces">
		{#snippet actions()}
			<Button
				variant="ghost"
				size="icon"
				icon={RefreshCw}
				disabled={busy}
				aria-label={t('common.reload')}
				title={t('common.reload')}
				onclick={requestReload}
			/>
			<Button variant="primary" icon={Save} loading={busy && loaded} disabled={!dirty} onclick={onSave}>
				{busy && loaded ? t('common.saving') : t('common.saveChanges')}
			</Button>
		{/snippet}
	</PageHeader>

	{#if !loaded}
		<LoadingState label={t('common.loading')} />
	{:else}
		<Section title={t('network.interfacesTitle')} description={t('network.interfacesDescription')}>
			<div class="iface-grid">
				<div class="iface-column">
					<h4>{t('network.wan')}</h4>
					<p class="hint">{t('network.wanHint')}</p>
					<label class="iface-row">
						<input
							type="checkbox"
							checked={wanInterfaces.includes('auto')}
							disabled={busy}
							onchange={(event) =>
								toggleWan('auto', (event.currentTarget as HTMLInputElement).checked)}
						/>
						<span class="iface-name">{t('network.autoDetect')}</span>
						<span class="iface-meta">auto</span>
					</label>
					{#each inventory as iface (iface.name)}
						<label class="iface-row">
							<input
								type="checkbox"
								checked={wanInterfaces.includes(iface.name)}
								disabled={busy}
								onchange={(event) =>
									toggleWan(iface.name, (event.currentTarget as HTMLInputElement).checked)}
							/>
							<span class="iface-name">{iface.name}</span>
							<span class="iface-meta">{ifaceMeta(iface)}</span>
						</label>
					{/each}
					{#if inventory.length === 0}
						<p class="empty">{t('network.noInterfaces')}</p>
					{/if}
				</div>

				<div class="iface-column">
					<h4>{t('network.lan')}</h4>
					<p class="hint">{t('network.lanHint')}</p>
					{#each inventory as iface (iface.name)}
						<label class="iface-row">
							<input
								type="checkbox"
								checked={lanInterfaces.includes(iface.name)}
								disabled={busy}
								onchange={(event) =>
									toggleLan(iface.name, (event.currentTarget as HTMLInputElement).checked)}
							/>
							<span class="iface-name">{iface.name}</span>
							<span class="iface-meta">{ifaceMeta(iface)}</span>
						</label>
					{/each}
					{#if inventory.length === 0}
						<p class="empty">{t('network.noInterfaces')}</p>
					{/if}
				</div>
			</div>
		</Section>

		<Section title={t('network.kernelTitle')} description={t('network.kernelHint')}>
			<div class="kernel-field">
				<label class="kernel-toggle">
					<input type="checkbox" bind:checked={autoConfigKernel} disabled={busy} />
					<span>{t('network.kernel')}</span>
				</label>
			</div>
		</Section>

		{#if dirty}
			<div class="sticky-actions">
				<div class="sticky-actions__copy">
					<strong>{t('common.unsavedChanges')}</strong>
					<span>{t('network.unsavedDescription')}</span>
				</div>
				<div class="inline-actions">
					<Button disabled={busy} onclick={requestReload}>{t('common.discard')}</Button>
					<Button variant="primary" icon={Save} loading={busy} onclick={onSave}
						>{t('common.saveChanges')}</Button
					>
				</div>
			</div>
		{/if}
	{/if}
</AppPage>

<ConfirmDialog
	bind:open={confirmReload}
	title={t('common.discardTitle')}
	description={t('common.discardDescription')}
	confirmLabel={t('common.discard')}
	cancelLabel={t('common.cancel')}
	onconfirm={confirmDiscard}
/>

<style>
	.iface-grid {
		display: grid;
		grid-template-columns: repeat(auto-fit, minmax(16rem, 1fr));
		gap: var(--space-6);
	}

	.iface-column h4 {
		margin: 0 0 var(--space-1);
		font-size: 0.85rem;
		font-weight: 650;
	}

	.hint,
	.empty {
		margin: 0 0 var(--space-3);
		color: var(--ink-muted);
		font-size: 0.78rem;
		line-height: 1.4;
	}

	.iface-row {
		display: grid;
		grid-template-columns: auto minmax(0, 1fr) auto;
		align-items: center;
		gap: var(--space-2);
		padding: var(--space-2) 0;
		border-bottom: 1px solid var(--line);
		font-size: 0.82rem;
	}

	.iface-row:last-child {
		border-bottom: 0;
	}

	.iface-name {
		font-family: var(--font-mono);
		font-size: 0.8rem;
	}

	.iface-meta {
		color: var(--ink-muted);
		font-size: 0.72rem;
		text-align: right;
		max-width: 14rem;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.kernel-toggle {
		display: inline-flex;
		align-items: center;
		gap: var(--space-2);
		font-size: 0.85rem;
	}

	.sticky-actions {
		position: sticky;
		bottom: var(--space-3);
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		justify-content: space-between;
		gap: var(--space-3);
		margin-top: var(--space-4);
		padding: var(--space-3) var(--space-4);
		border: 1px solid var(--line);
		border-radius: var(--radius-md);
		background: var(--surface);
		box-shadow: var(--shadow-sm, 0 1px 2px rgb(0 0 0 / 6%));
	}

	.sticky-actions__copy {
		display: flex;
		flex-direction: column;
		gap: 0.15rem;
		font-size: 0.8rem;
	}

	.sticky-actions__copy span {
		color: var(--ink-muted);
	}

	.inline-actions {
		display: flex;
		gap: var(--space-2);
	}
</style>
