<script lang="ts">
	import { onMount } from 'svelte';
	import { beforeNavigate } from '$app/navigation';
	import { RefreshCw, Save } from '@lucide/svelte';
	import {
		getDns,
		putDns,
		ApiClientError,
		isSessionRedirectPending,
		type RoutingRuleDto
	} from '$lib/api';
	import { apiErrorText, t } from '$lib/i18n.svelte';
	import Button from '$lib/components/ui/Button.svelte';
	import ConfirmDialog from '$lib/components/ui/ConfirmDialog.svelte';
	import Field from '$lib/components/ui/Field.svelte';
	import LoadingState from '$lib/components/ui/LoadingState.svelte';
	import Notice from '$lib/components/ui/Notice.svelte';
	import PageHeader from '$lib/components/ui/PageHeader.svelte';
	import Section from '$lib/components/ui/Section.svelte';
	import NamedEndpointEditor, {
		type NamedEndpoint
	} from '$lib/components/features/NamedEndpointEditor.svelte';
	import RoutingRuleEditor from '$lib/components/features/RoutingRuleEditor.svelte';

	let upstreams = $state<NamedEndpoint[]>([]);
	let rules = $state<RoutingRuleDto[]>([]);
	let fallback = $state('alidns');
	let error = $state('');
	let message = $state('');
	let loaded = $state(false);
	let busy = $state(false);
	let savedSnapshot = $state('');
	let confirmReload = $state(false);

	function snapshot() {
		return JSON.stringify({ upstreams, rules, fallback });
	}

	async function load() {
		busy = true;
		error = '';
		try {
			const document = await getDns();
			upstreams = document.upstreams.map((upstream) => ({
				name: upstream.name,
				value: upstream.address
			}));
			rules = document.rules.map((rule) => ({
				expression: rule.expression,
				outbound: rule.upstream,
				enabled: rule.enabled
			}));
			fallback = document.fallback;
			savedSnapshot = snapshot();
			confirmReload = false;
		} catch (cause) {
			error = cause instanceof ApiClientError ? apiErrorText(cause) : t('dns.loadFailed');
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
		return () => {
			delete document.documentElement.dataset.chaosUnsaved;
		};
	});

	function requestReload() {
		if (dirty) confirmReload = true;
		else void load();
	}

	async function onSave() {
		error = '';
		message = '';
		const names = upstreams.map((upstream) => upstream.name.trim()).filter(Boolean);
		const nameSet = new Set(names);
		if (!upstreams.length || upstreams.some((upstream) => !upstream.name.trim() || !upstream.value.trim())) {
			error = t('dns.upstreamRequired');
			return;
		}
		if (nameSet.size !== names.length) {
			error = t('dns.duplicateUpstream');
			return;
		}
		if (!fallback.trim() || !nameSet.has(fallback.trim())) {
			error = t('dns.invalidFallback');
			return;
		}
		if (
			rules.some(
				(rule) =>
					rule.enabled &&
					(!rule.expression.trim() || !rule.outbound.trim() || !nameSet.has(rule.outbound.trim()))
			)
		) {
			error = t('dns.invalidRule');
			return;
		}

		busy = true;
		try {
			const document = await putDns({
				upstreams: upstreams.map((upstream) => ({
					name: upstream.name.trim(),
					address: upstream.value.trim()
				})),
				rules: rules.map((rule) => ({
					expression: rule.expression.trim(),
					upstream: rule.outbound.trim(),
					enabled: rule.enabled
				})),
				fallback: fallback.trim()
			});
			upstreams = document.upstreams.map((upstream) => ({ name: upstream.name, value: upstream.address }));
			rules = document.rules.map((rule) => ({
				expression: rule.expression,
				outbound: rule.upstream,
				enabled: rule.enabled
			}));
			fallback = document.fallback;
			savedSnapshot = snapshot();
			message = t('dns.saved');
		} catch (cause) {
			error = cause instanceof ApiClientError ? apiErrorText(cause) : t('dns.saveFailed');
		} finally {
			busy = false;
		}
	}

	function onBeforeUnload(event: BeforeUnloadEvent) {
		if (!dirty || isSessionRedirectPending()) return;
		event.preventDefault();
		event.returnValue = '';
	}

	const upstreamNames = $derived(upstreams.map((upstream) => upstream.name.trim()).filter(Boolean));
	const dirty = $derived(loaded && snapshot() !== savedSnapshot);
</script>

<svelte:window onbeforeunload={onBeforeUnload} />

<div class="page-stack">
	<PageHeader title={t('dns.title')} description={t('dns.subtitle')} meta="network / dns">
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

	{#if error}<Notice tone="error" message={error} ondismiss={() => (error = '')} />{/if}
	{#if message}<Notice tone="success" message={message} ondismiss={() => (message = '')} />{/if}

	{#if !loaded}
		<LoadingState label={t('common.loading')} />
	{:else}
		<Section title={t('dns.upstreams')} description={t('dns.upstreamsDescription')} count={upstreams.length}>
			<NamedEndpointEditor
				bind:items={upstreams}
				nameLabel={t('dns.name')}
				valueLabel={t('dns.address')}
				namePlaceholder="alidns"
				valuePlaceholder="https://dns.alidns.com/dns-query"
				addLabel={t('dns.addUpstream')}
				{busy}
			/>
		</Section>

		<Section title={t('dns.defaultsTitle')} description={t('dns.hint')}>
			<div class="fallback-field">
				<Field label={t('dns.fallback')} forId="dns-fallback">
					<select id="dns-fallback" bind:value={fallback} disabled={busy}>
						{#if fallback && !upstreamNames.includes(fallback)}<option value={fallback}>{fallback}</option>{/if}
						{#each upstreamNames as name (name)}<option value={name}>{name}</option>{/each}
					</select>
				</Field>
			</div>
		</Section>

		<Section title={t('dns.rules')} description={t('dns.rulesDescription')} count={rules.length}>
			<RoutingRuleEditor bind:rules outbounds={upstreamNames} {busy} />
		</Section>

		{#if dirty}
			<div class="sticky-actions">
				<div>
					<strong>{t('common.unsavedChanges')}</strong>
					<span>{t('dns.unsavedDescription')}</span>
				</div>
				<div class="inline-actions">
					<Button disabled={busy} onclick={requestReload}>{t('common.discard')}</Button>
					<Button variant="primary" icon={Save} loading={busy} onclick={onSave}>{t('common.saveChanges')}</Button>
				</div>
			</div>
		{/if}
	{/if}
</div>

<ConfirmDialog
	bind:open={confirmReload}
	title={t('common.discardTitle')}
	description={t('common.discardDescription')}
	confirmLabel={t('common.discard')}
	cancelLabel={t('common.cancel')}
	danger
	onconfirm={load}
/>

<style>
	.fallback-field {
		width: min(22rem, 100%);
	}

	.sticky-actions > div:first-child {
		display: flex;
		min-width: 0;
		flex-direction: column;
	}

	.sticky-actions strong {
		font-size: 0.8rem;
	}

	.sticky-actions span {
		color: var(--ink-muted);
		font-size: 0.7rem;
	}
</style>
