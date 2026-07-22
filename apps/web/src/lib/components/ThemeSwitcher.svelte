<script lang="ts">
	import { Sun, Moon, Monitor } from '@lucide/svelte';
	import { getTheme, setTheme, type Theme } from '$lib/theme.svelte';

	const theme = $derived(getTheme());

	function selectTheme(newTheme: Theme) {
		setTheme(newTheme);
	}

	const themes: { value: Theme; icon: typeof Sun; label: string }[] = [
		{ value: 'light', icon: Sun, label: 'Light' },
		{ value: 'dark', icon: Moon, label: 'Dark' },
		{ value: 'system', icon: Monitor, label: 'System' }
	];
</script>

<div class="theme-switcher">
	{#each themes as t}
		<button
			class="theme-btn"
			class:active={theme === t.value}
			onclick={() => selectTheme(t.value)}
			title={t.label}
			aria-label={t.label}
		>
			<t.icon size={14} />
		</button>
	{/each}
</div>

<style>
	.theme-switcher {
		display: inline-flex;
		align-items: center;
		gap: 2px;
		padding: 2px;
		border: 1px solid var(--line);
		border-radius: var(--radius-md);
		background: var(--surface-subtle);
	}

	.theme-btn {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		width: 1.75rem;
		height: 1.75rem;
		border: none;
		border-radius: var(--radius-sm);
		background: transparent;
		color: var(--ink-faint);
		cursor: pointer;
		transition: all var(--motion-fast);
	}

	.theme-btn:hover {
		color: var(--ink);
		background: var(--surface-hover);
	}

	.theme-btn.active {
		color: var(--ink-inverse);
		background: var(--surface-inverse);
	}
</style>
