<script lang="ts">
	import { Eye, EyeOff } from '@lucide/svelte';

	let {
		value = $bindable(''),
		disabled = false,
		autocomplete = 'current-password',
		minlength = 8,
		id,
		placeholder = '',
		label = 'Password'
	} = $props<{
		value?: string;
		disabled?: boolean;
		autocomplete?: string;
		minlength?: number;
		id?: string;
		placeholder?: string;
		label?: string;
	}>();

	let visible = $state(false);
</script>

<div class="password-input">
	<input
		{id}
		type={visible ? 'text' : 'password'}
		bind:value
		{disabled}
		{autocomplete}
		{minlength}
		{placeholder}
		required
	/>
	<button
		type="button"
		aria-label={visible ? `Hide ${label}` : `Show ${label}`}
		title={visible ? `Hide ${label}` : `Show ${label}`}
		disabled={disabled}
		onclick={() => (visible = !visible)}
	>
		{#if visible}
			<EyeOff size={16} strokeWidth={1.8} aria-hidden="true" />
		{:else}
			<Eye size={16} strokeWidth={1.8} aria-hidden="true" />
		{/if}
	</button>
</div>

<style>
	.password-input {
		position: relative;
	}

	input {
		padding-right: 2.6rem;
	}

	button {
		position: absolute;
		right: 0.38rem;
		top: 50%;
		display: inline-grid;
		place-items: center;
		width: 1.8rem;
		height: 1.8rem;
		padding: 0;
		border: 0;
		border-radius: var(--radius-sm);
		background: transparent;
		color: var(--ink-muted);
		transform: translateY(-50%);
	}

	button:hover:not(:disabled) {
		background: var(--surface-subtle);
		color: var(--ink);
	}
</style>
