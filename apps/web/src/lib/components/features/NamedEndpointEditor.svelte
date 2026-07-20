<script lang="ts">
	import { ArrowDown, ArrowUp, Copy, Plus, Trash2 } from '@lucide/svelte';
	import { t } from '$lib/i18n.svelte';
	import Button from '$lib/components/ui/Button.svelte';

	export type NamedEndpoint = { name: string; value: string };

	let {
		items = $bindable(),
		nameLabel,
		valueLabel,
		namePlaceholder = '',
		valuePlaceholder = '',
		addLabel,
		busy = false,
		onchange
	}: {
		items: NamedEndpoint[];
		nameLabel: string;
		valueLabel: string;
		namePlaceholder?: string;
		valuePlaceholder?: string;
		addLabel: string;
		busy?: boolean;
		onchange?: () => void;
	} = $props();

	function emit(next: NamedEndpoint[]) {
		items = next;
		onchange?.();
	}

	function update(index: number, patch: Partial<NamedEndpoint>) {
		emit(items.map((item, itemIndex) => (itemIndex === index ? { ...item, ...patch } : item)));
	}

	function add() {
		emit([...items, { name: '', value: '' }]);
	}

	function duplicate(index: number) {
		const next = [...items];
		next.splice(index + 1, 0, { ...items[index], name: `${items[index].name}_copy` });
		emit(next);
	}

	function remove(index: number) {
		emit(items.filter((_, itemIndex) => itemIndex !== index));
	}

	function move(index: number, direction: -1 | 1) {
		const target = index + direction;
		if (target < 0 || target >= items.length) return;
		const next = [...items];
		[next[index], next[target]] = [next[target], next[index]];
		emit(next);
	}
</script>

<div class="endpoint-list">
	{#each items as item, index (index)}
		<div class="endpoint-row">
			<span class="index">{String(index + 1).padStart(2, '0')}</span>
			<label>
				<span>{nameLabel}</span>
				<input
					type="text"
					value={item.name}
					class:invalid={!item.name.trim()}
					placeholder={namePlaceholder}
					disabled={busy}
					oninput={(event) => update(index, { name: (event.currentTarget as HTMLInputElement).value })}
				/>
			</label>
			<label class="value-field">
				<span>{valueLabel}</span>
				<input
					type="text"
					value={item.value}
					class:invalid={!item.value.trim()}
					placeholder={valuePlaceholder}
					disabled={busy}
					oninput={(event) => update(index, { value: (event.currentTarget as HTMLInputElement).value })}
				/>
			</label>
			<div class="endpoint-actions">
				<Button
					variant="ghost"
					size="icon"
					icon={ArrowUp}
					disabled={busy || index === 0}
					aria-label={t('flow.moveUp')}
					title={t('flow.moveUp')}
					onclick={() => move(index, -1)}
				/>
				<Button
					variant="ghost"
					size="icon"
					icon={ArrowDown}
					disabled={busy || index === items.length - 1}
					aria-label={t('flow.moveDown')}
					title={t('flow.moveDown')}
					onclick={() => move(index, 1)}
				/>
				<Button
					variant="ghost"
					size="icon"
					icon={Copy}
					disabled={busy}
					aria-label={t('common.duplicate')}
					title={t('common.duplicate')}
					onclick={() => duplicate(index)}
				/>
				<Button
					variant="ghost"
					size="icon"
					icon={Trash2}
					disabled={busy}
					aria-label={t('common.delete')}
					title={t('common.delete')}
					onclick={() => remove(index)}
				/>
			</div>
		</div>
	{/each}
</div>

<div class="add-row">
	<Button icon={Plus} disabled={busy} onclick={add}>{addLabel}</Button>
</div>

<style>
	.endpoint-list {
		display: flex;
		flex-direction: column;
		gap: var(--space-3);
	}

	.endpoint-row {
		display: grid;
		grid-template-columns: 2rem minmax(8rem, 0.7fr) minmax(14rem, 1.3fr) auto;
		align-items: end;
		gap: var(--space-3);
		padding: var(--space-3);
		border: 1px solid var(--line);
		border-radius: var(--radius-md);
	}

	.index {
		align-self: center;
		color: var(--ink-faint);
		font-family: var(--font-mono);
		font-size: 0.68rem;
	}

	label {
		display: flex;
		min-width: 0;
		flex-direction: column;
		gap: 0.32rem;
	}

	label > span {
		color: var(--ink-muted);
		font-size: 0.68rem;
		font-weight: 650;
	}

	.value-field input {
		font-family: var(--font-mono);
		font-size: 0.76rem;
	}

	.invalid {
		border-color: var(--ink);
		background: var(--danger-surface);
	}

	.endpoint-actions {
		display: flex;
		align-items: center;
		gap: var(--space-1);
	}

	.add-row {
		padding-top: var(--space-3);
	}

	@media (max-width: 760px) {
		.endpoint-row {
			grid-template-columns: 2rem minmax(0, 1fr);
		}

		.endpoint-row label,
		.endpoint-actions {
			grid-column: 2;
		}

		.index {
			grid-row: 1 / span 3;
			align-self: start;
			padding-top: var(--space-2);
		}

		.endpoint-actions {
			justify-content: flex-end;
		}
	}
</style>
