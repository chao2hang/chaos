<script lang="ts">
	import { Check, Circle, CircleAlert, Minus } from '@lucide/svelte';

	let {
		label,
		tone = 'neutral',
		size = 'md'
	} = $props<{
		label: string;
		tone?: 'positive' | 'negative' | 'warning' | 'neutral';
		size?: 'sm' | 'md';
	}>();

	const Icon = $derived(
		tone === 'positive' ? Check : tone === 'negative' ? CircleAlert : tone === 'warning' ? Minus : Circle
	);
</script>

<span class="status {tone} {size}">
	<Icon size={size === 'sm' ? 12 : 14} strokeWidth={2} aria-hidden="true" />
	<span>{label}</span>
</span>

<style>
	.status {
		display: inline-flex;
		align-items: center;
		gap: 0.32rem;
		width: fit-content;
		color: var(--ink);
		font-size: 0.75rem;
		font-weight: 650;
		white-space: nowrap;
	}

	.neutral,
	.warning {
		color: var(--ink-muted);
	}

	.negative {
		text-decoration: underline;
		text-decoration-style: dotted;
		text-underline-offset: 0.2em;
	}

	.sm {
		font-size: 0.68rem;
	}
</style>
