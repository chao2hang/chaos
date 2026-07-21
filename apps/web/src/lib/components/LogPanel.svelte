<script lang="ts">
	import { onMount, onDestroy } from 'svelte';
	import { Pause, Play, Trash2 } from '@lucide/svelte';
	import { getLogs } from '$lib/api';
	import { t } from '$lib/i18n.svelte';
	import Button from '$lib/components/ui/Button.svelte';

	let logs = $state<string[]>([]);
	let autoScroll = $state(true);
	let streaming = $state(false);
	let error = $state('');
	let logContainer: HTMLDivElement | undefined = $state();

	let eventSource: EventSource | null = null;

	async function loadLogs() {
		try {
			const response = await getLogs(200);
			logs = response.lines;
			error = '';
		} catch {
			error = t('logs.loadFailed');
		}
	}

	function startStreaming() {
		if (eventSource) return;
		streaming = true;
		eventSource = new EventSource('/api/v1/runtime/logs/stream');
		eventSource.onmessage = (event) => {
			logs = [...logs.slice(-499), event.data];
			if (autoScroll && logContainer) {
				logContainer.scrollTop = logContainer.scrollHeight;
			}
		};
		eventSource.onerror = () => {
			stopStreaming();
		};
	}

	function stopStreaming() {
		streaming = false;
		if (eventSource) {
			eventSource.close();
			eventSource = null;
		}
	}

	function toggleStreaming() {
		if (streaming) {
			stopStreaming();
		} else {
			startStreaming();
		}
	}

	function clearLogs() {
		logs = [];
	}

	function scrollToBottom() {
		if (logContainer) {
			logContainer.scrollTop = logContainer.scrollHeight;
		}
	}

	onMount(() => {
		void loadLogs();
	});

	onDestroy(() => {
		stopStreaming();
	});
</script>

<div class="log-panel">
	<div class="log-header">
		<span class="log-title">{t('logs.title')}</span>
		<div class="log-actions">
			<Button
				variant="ghost"
				size="icon"
				icon={streaming ? Pause : Play}
				title={streaming ? t('logs.pause') : t('logs.stream')}
				onclick={toggleStreaming}
			/>
			<Button
				variant="ghost"
				size="icon"
				icon={Trash2}
				title={t('logs.clear')}
				onclick={clearLogs}
			/>
		</div>
	</div>
	<div class="log-container" bind:this={logContainer}>
		{#if error}
			<p class="log-error">{error}</p>
		{:else if logs.length === 0}
			<p class="log-empty">{t('logs.empty')}</p>
		{:else}
			{#each logs as line, i (i)}
				<div class="log-line">{line}</div>
			{/each}
		{/if}
	</div>
	<div class="log-footer">
		<label class="auto-scroll">
			<input type="checkbox" bind:checked={autoScroll} />
			{t('logs.autoScroll')}
		</label>
		{#if streaming}
			<span class="streaming-indicator">{t('logs.streaming')}</span>
		{/if}
	</div>
</div>

<style>
	.log-panel {
		display: flex;
		flex-direction: column;
		border: 1px solid var(--line);
		border-radius: var(--radius-lg);
		background: var(--surface);
		overflow: hidden;
	}

	.log-header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: var(--space-2) var(--space-3);
		border-bottom: 1px solid var(--line);
		background: var(--surface-subtle);
	}

	.log-title {
		font-size: 0.75rem;
		font-weight: 650;
		color: var(--ink-muted);
		text-transform: uppercase;
	}

	.log-actions {
		display: flex;
		gap: var(--space-1);
	}

	.log-container {
		height: 200px;
		overflow-y: auto;
		padding: var(--space-2);
		font-family: var(--font-mono);
		font-size: 0.72rem;
		line-height: 1.5;
		background: var(--canvas);
	}

	.log-line {
		white-space: pre-wrap;
		word-break: break-all;
		color: var(--ink-muted);
	}

	.log-line:hover {
		color: var(--ink);
		background: var(--surface-hover);
	}

	.log-error {
		color: var(--ink);
		font-style: italic;
	}

	.log-empty {
		color: var(--ink-faint);
		font-style: italic;
	}

	.log-footer {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: var(--space-2) var(--space-3);
		border-top: 1px solid var(--line);
		background: var(--surface-subtle);
	}

	.auto-scroll {
		display: flex;
		align-items: center;
		gap: var(--space-2);
		font-size: 0.72rem;
		color: var(--ink-muted);
		cursor: pointer;
	}

	.auto-scroll input {
		width: 0.875rem;
		height: 0.875rem;
	}

	.streaming-indicator {
		font-size: 0.72rem;
		color: var(--ink-faint);
		animation: pulse 1.5s ease-in-out infinite;
	}

	@keyframes pulse {
		0%, 100% { opacity: 1; }
		50% { opacity: 0.5; }
	}
</style>
