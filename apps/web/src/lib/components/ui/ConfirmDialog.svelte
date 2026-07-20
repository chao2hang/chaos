<script lang="ts">
	import { TriangleAlert, X } from '@lucide/svelte';
	import { tick } from 'svelte';
	import Button from './Button.svelte';

	let {
		open = $bindable(false),
		title,
		description,
		confirmLabel = 'Confirm',
		cancelLabel = 'Cancel',
		busy = false,
		danger = false,
		onconfirm,
		oncancel
	} = $props<{
		open?: boolean;
		title: string;
		description: string;
		confirmLabel?: string;
		cancelLabel?: string;
		busy?: boolean;
		danger?: boolean;
		onconfirm: () => void | Promise<void>;
		oncancel?: () => void;
	}>();
	let dialogElement = $state<HTMLDivElement | null>(null);

	function close() {
		if (!busy) {
			open = false;
			oncancel?.();
		}
	}

	function onKeydown(event: KeyboardEvent) {
		if (event.key === 'Escape') close();
		if (event.key !== 'Tab' || !dialogElement) return;
		const focusable = Array.from(
			dialogElement.querySelectorAll<HTMLElement>('button:not(:disabled), [href], input:not(:disabled), select:not(:disabled), textarea:not(:disabled), [tabindex]:not([tabindex="-1"])')
		);
		if (!focusable.length) return;
		const first = focusable[0];
		const last = focusable.at(-1)!;
		if (event.shiftKey && document.activeElement === first) {
			event.preventDefault();
			last.focus();
		} else if (!event.shiftKey && document.activeElement === last) {
			event.preventDefault();
			first.focus();
		}
	}

	$effect(() => {
		if (!open || typeof document === 'undefined') return;
		const previous = document.activeElement as HTMLElement | null;
		const previousOverflow = document.body.style.overflow;
		document.body.style.overflow = 'hidden';
		void tick().then(() => dialogElement?.querySelector<HTMLElement>('button:not(:disabled)')?.focus());
		return () => {
			document.body.style.overflow = previousOverflow;
			previous?.focus();
		};
	});
</script>

<svelte:window onkeydown={open ? onKeydown : undefined} />

{#if open}
	<div class="dialog-layer" role="presentation">
		<button class="backdrop" type="button" aria-label={cancelLabel} onclick={close}></button>
		<div bind:this={dialogElement} class="dialog" role="alertdialog" aria-modal="true" aria-labelledby="dialog-title" aria-describedby="dialog-description">
			<header>
				<span class="dialog-icon"><TriangleAlert size={17} strokeWidth={1.8} aria-hidden="true" /></span>
				<div>
					<h2 id="dialog-title">{title}</h2>
					<p id="dialog-description">{description}</p>
				</div>
				<button class="close" type="button" aria-label={cancelLabel} title={cancelLabel} onclick={close}>
					<X size={16} strokeWidth={1.8} aria-hidden="true" />
				</button>
			</header>
			<footer>
				<Button onclick={close} disabled={busy}>{cancelLabel}</Button>
				<Button variant={danger ? 'danger' : 'primary'} loading={busy} onclick={onconfirm}>
					{confirmLabel}
				</Button>
			</footer>
		</div>
	</div>
{/if}

<style>
	.dialog-layer {
		position: fixed;
		inset: 0;
		z-index: 100;
		display: grid;
		place-items: center;
		padding: var(--space-4);
	}

	.backdrop {
		position: absolute;
		inset: 0;
		width: 100%;
		height: 100%;
		padding: 0;
		border: 0;
		background: var(--overlay);
		cursor: default;
	}

	.dialog {
		position: relative;
		z-index: 1;
		width: min(28rem, 100%);
		border: 1px solid var(--ink);
		border-radius: var(--radius-lg);
		background: var(--surface);
		box-shadow: var(--shadow-float);
	}

	header {
		display: grid;
		grid-template-columns: auto 1fr auto;
		gap: var(--space-3);
		padding: var(--space-5);
	}

	.dialog-icon {
		display: inline-grid;
		place-items: center;
		width: 2rem;
		height: 2rem;
		border: 1px solid var(--ink);
		border-radius: var(--radius-md);
		background: var(--danger-surface);
	}

	h2 {
		margin: 0;
		font-size: 0.95rem;
		font-weight: 700;
	}

	p {
		margin: var(--space-1) 0 0;
		color: var(--ink-muted);
		font-size: 0.8rem;
	}

	.close {
		display: inline-grid;
		place-items: center;
		width: 1.75rem;
		height: 1.75rem;
		padding: 0;
		border: 0;
		border-radius: var(--radius-sm);
		background: transparent;
		color: var(--ink-muted);
	}

	.close:hover {
		background: var(--surface-subtle);
		color: var(--ink);
	}

	footer {
		display: flex;
		justify-content: flex-end;
		gap: var(--space-2);
		padding: var(--space-3) var(--space-5);
		border-top: 1px solid var(--line);
		background: var(--surface-subtle);
	}
</style>
