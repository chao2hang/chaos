<script lang="ts">
	import { onMount } from 'svelte';
	import { CheckCircle2, XCircle, AlertTriangle } from '@lucide/svelte';
	import { getDiagnostics, type DiagnosticsResponse } from '$lib/api';
	import { t } from '$lib/i18n.svelte';

	let diagnostics = $state<DiagnosticsResponse | null>(null);
	let error = $state('');
	let loading = $state(true);

	onMount(async () => {
		try {
			diagnostics = await getDiagnostics();
		} catch {
			error = t('diagnostics.loadFailed');
		} finally {
			loading = false;
		}
	});

	function statusIcon(ok: boolean) {
		return ok ? CheckCircle2 : XCircle;
	}
</script>

<div class="diagnostics">
	<h3>{t('diagnostics.title')}</h3>
	
	{#if loading}
		<p class="loading">{t('common.loading')}</p>
	{:else if error}
		<p class="error">{error}</p>
	{:else if diagnostics}
		<dl class="diag-list">
			<div class="diag-item">
				<dt>{t('diagnostics.kernel')}</dt>
				<dd>
					<code>{diagnostics.kernel_version}</code>
					{#if diagnostics.kernel_ok}
						<CheckCircle2 size={14} class="ok" />
					{:else}
						<XCircle size={14} class="bad" />
					{/if}
				</dd>
			</div>
			
			<div class="diag-item">
				<dt>{t('diagnostics.ebpf')}</dt>
				<dd>
					{diagnostics.ebpf_supported ? t('common.supported') : t('common.unsupported')}
					{#if diagnostics.ebpf_supported}
						<CheckCircle2 size={14} class="ok" />
					{:else}
						<XCircle size={14} class="bad" />
					{/if}
				</dd>
			</div>
			
			<div class="diag-item">
				<dt>{t('diagnostics.cgroup2')}</dt>
				<dd>
					{diagnostics.cgroup2_mounted ? t('common.mounted') : t('common.notMounted')}
					{#if diagnostics.cgroup2_mounted}
						<CheckCircle2 size={14} class="ok" />
					{:else}
						<AlertTriangle size={14} class="warn" />
					{/if}
				</dd>
			</div>
			
			<div class="diag-item">
				<dt>{t('diagnostics.bpffs')}</dt>
				<dd>
					{diagnostics.bpf_fs_mounted ? t('common.mounted') : t('common.notMounted')}
					{#if diagnostics.bpf_fs_mounted}
						<CheckCircle2 size={14} class="ok" />
					{:else}
						<AlertTriangle size={14} class="warn" />
					{/if}
				</dd>
			</div>
			
			<div class="diag-item">
				<dt>{t('diagnostics.ipForward')}</dt>
				<dd>
					{diagnostics.ip_forward ? t('common.enabled') : t('common.disabled')}
					{#if diagnostics.ip_forward}
						<CheckCircle2 size={14} class="ok" />
					{:else}
						<AlertTriangle size={14} class="warn" />
					{/if}
				</dd>
			</div>
			
			<div class="diag-item">
				<dt>{t('diagnostics.interfaces')}</dt>
				<dd>
					<code>{diagnostics.interfaces.join(', ') || t('common.none')}</code>
				</dd>
			</div>
			
			{#if diagnostics.dae_binary_version}
				<div class="diag-item">
					<dt>{t('diagnostics.daeVersion')}</dt>
					<dd><code>{diagnostics.dae_binary_version}</code></dd>
				</div>
			{/if}
			
			<div class="diag-item">
				<dt>{t('diagnostics.permissions')}</dt>
				<dd class="perms">
					<span class:ok={diagnostics.permissions.root} class:bad={!diagnostics.permissions.root}>
						root: {diagnostics.permissions.root ? '✓' : '✗'}
					</span>
					<span class:ok={diagnostics.permissions.cap_net_admin} class:bad={!diagnostics.permissions.cap_net_admin}>
						NET_ADMIN: {diagnostics.permissions.cap_net_admin ? '✓' : '✗'}
					</span>
					<span class:ok={diagnostics.permissions.cap_bpf} class:bad={!diagnostics.permissions.cap_bpf}>
						BPF: {diagnostics.permissions.cap_bpf ? '✓' : '✗'}
					</span>
				</dd>
			</div>
		</dl>
	{/if}
</div>

<style>
	.diagnostics {
		padding: var(--space-4);
	}

	h3 {
		margin: 0 0 var(--space-4);
		font-size: 0.85rem;
		font-weight: 650;
	}

	.loading,
	.error {
		color: var(--ink-muted);
		font-size: 0.8rem;
	}

	.error {
		color: var(--ink);
	}

	.diag-list {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
		margin: 0;
	}

	.diag-item {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: var(--space-4);
		padding: var(--space-2) 0;
		border-bottom: 1px solid var(--line);
	}

	.diag-item:last-child {
		border-bottom: 0;
	}

	dt {
		color: var(--ink-muted);
		font-size: 0.75rem;
	}

	dd {
		display: flex;
		align-items: center;
		gap: var(--space-2);
		margin: 0;
		font-size: 0.78rem;
	}

	dd code {
		font-size: 0.72rem;
	}

	.ok {
		color: var(--ink);
	}

	.bad {
		color: var(--ink-muted);
	}

	:global(.warn) {
		color: var(--ink-faint);
	}

	.perms {
		display: flex;
		gap: var(--space-3);
		font-family: var(--font-mono);
		font-size: 0.7rem;
	}

	.perms .ok {
		color: var(--ink);
	}

	.perms .bad {
		color: var(--ink-faint);
		text-decoration: line-through;
	}
</style>
