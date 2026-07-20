<script lang="ts">
	let {
		checked = $bindable(false),
		label,
		disabled = false,
		onchange
	} = $props<{
		checked?: boolean;
		label: string;
		disabled?: boolean;
		onchange?: (checked: boolean) => void;
	}>();

	function handleChange(event: Event) {
		checked = (event.currentTarget as HTMLInputElement).checked;
		onchange?.(checked);
	}
</script>

<label class="toggle" class:disabled>
	<input type="checkbox" {checked} {disabled} onchange={handleChange} />
	<span class="track" aria-hidden="true"><span class="thumb"></span></span>
	<span class="label">{label}</span>
</label>

<style>
	.toggle {
		display: inline-flex;
		align-items: center;
		gap: var(--space-2);
		color: var(--ink-muted);
		font-size: 0.75rem;
		font-weight: 600;
		cursor: pointer;
	}

	.toggle.disabled {
		cursor: not-allowed;
		opacity: 0.5;
	}

	input {
		position: absolute;
		opacity: 0;
		pointer-events: none;
	}

	.track {
		position: relative;
		width: 2rem;
		height: 1.1rem;
		border: 1px solid var(--line-strong);
		border-radius: 999px;
		background: var(--surface);
		transition: background 130ms ease, border-color 130ms ease;
	}

	.thumb {
		position: absolute;
		top: 0.15rem;
		left: 0.15rem;
		width: 0.68rem;
		height: 0.68rem;
		border-radius: 50%;
		background: var(--ink-muted);
		transition: transform 130ms ease, background 130ms ease;
	}

	input:checked + .track {
		border-color: var(--ink);
		background: var(--surface-inverse);
	}

	input:checked + .track .thumb {
		transform: translateX(0.88rem);
		background: var(--ink-inverse);
	}

	input:focus-visible + .track {
		outline: 2px solid var(--focus);
		outline-offset: 2px;
	}
</style>
