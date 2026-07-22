<script lang="ts">
	import { Check, ChevronDown, Search, X } from '@lucide/svelte';

	export type MultiSelectOption = {
		value: string;
		label: string;
		meta?: string;
	};

	let {
		values = [],
		options,
		label = 'Select',
		placeholder = 'Select…',
		searchPlaceholder = 'Search…',
		emptyLabel = 'No results',
		disabled = false,
		id = undefined,
		onChange
	}: {
		values?: string[];
		options: MultiSelectOption[];
		label?: string;
		placeholder?: string;
		searchPlaceholder?: string;
		emptyLabel?: string;
		disabled?: boolean;
		id?: string;
		onChange: (next: string[]) => void;
	} = $props();

	let open = $state(false);
	let query = $state('');
	let rootEl = $state<HTMLDivElement | null>(null);

	const normalizedValues = $derived(
		values.map((value) => value.toLowerCase()).filter(Boolean)
	);

	const selectedOptions = $derived(
		normalizedValues.map((value) => {
			const option = options.find((item) => item.value.toLowerCase() === value);
			return option ?? { value, label: value, meta: value };
		})
	);

	const filteredOptions = $derived.by(() => {
		const q = query.trim().toLowerCase();
		if (!q) return options;
		return options.filter((option) => {
			const hay = `${option.label} ${option.value} ${option.meta ?? ''}`.toLowerCase();
			return hay.includes(q);
		});
	});

	function commit(next: string[]) {
		const unique: string[] = [];
		const seen = new Set<string>();
		for (const value of next) {
			const code = value.trim().toLowerCase();
			if (!code || seen.has(code)) continue;
			seen.add(code);
			unique.push(code);
		}
		onChange(unique);
	}

	function toggle(value: string) {
		const code = value.toLowerCase();
		if (normalizedValues.includes(code)) {
			commit(normalizedValues.filter((item) => item !== code));
		} else {
			commit([...normalizedValues, code]);
		}
	}

	function remove(value: string) {
		commit(normalizedValues.filter((item) => item !== value.toLowerCase()));
	}

	function clearAll() {
		commit([]);
	}

	function onDocumentPointerDown(event: PointerEvent) {
		if (!open || !rootEl) return;
		const target = event.target as Node | null;
		if (target && !rootEl.contains(target)) {
			open = false;
			query = '';
		}
	}

	function onKeydown(event: KeyboardEvent) {
		if (event.key === 'Escape' && open) {
			event.preventDefault();
			open = false;
			query = '';
		}
	}

	$effect(() => {
		if (!open) return;
		document.addEventListener('pointerdown', onDocumentPointerDown, true);
		document.addEventListener('keydown', onKeydown);
		return () => {
			document.removeEventListener('pointerdown', onDocumentPointerDown, true);
			document.removeEventListener('keydown', onKeydown);
		};
	});
</script>

<div class="multi-select" class:open class:disabled bind:this={rootEl}>
	<div class="control" class:open>
		<div class="chips-area">
			{#if selectedOptions.length}
				<div class="chips">
					{#each selectedOptions as option (option.value)}
						<span class="chip">
							<span class="chip-label" title={`${option.label} (${option.value})`}>
								{option.label}
							</span>
							<button
								type="button"
								class="chip-remove"
								disabled={disabled}
								aria-label={`Remove ${option.label}`}
								onclick={() => remove(option.value)}
							>
								<X size={11} strokeWidth={2} aria-hidden="true" />
							</button>
						</span>
					{/each}
				</div>
			{:else}
				<span class="placeholder">{placeholder}</span>
			{/if}
		</div>
		<div class="actions">
			{#if selectedOptions.length}
				<button
					type="button"
					class="icon-btn"
					disabled={disabled}
					aria-label="Clear"
					onclick={clearAll}
				>
					<X size={13} strokeWidth={1.8} aria-hidden="true" />
				</button>
			{/if}
			<span class="count" class:empty={!selectedOptions.length}>{selectedOptions.length}</span>
			<button
				{id}
				type="button"
				class="icon-btn chevron-btn"
				aria-haspopup="listbox"
				aria-expanded={open}
				aria-label={label}
				disabled={disabled}
				onclick={() => {
					if (disabled) return;
					open = !open;
					if (!open) query = '';
				}}
			>
				<ChevronDown size={14} strokeWidth={1.8} aria-hidden="true" />
			</button>
		</div>
	</div>

	{#if open}
		<div class="panel" role="listbox" aria-multiselectable="true" aria-label={label}>
			<label class="search">
				<Search size={13} strokeWidth={1.8} aria-hidden="true" />
				<input
					type="search"
					value={query}
					placeholder={searchPlaceholder}
					disabled={disabled}
					oninput={(event) => (query = (event.currentTarget as HTMLInputElement).value)}
				/>
			</label>
			<div class="options">
				{#each filteredOptions as option (option.value)}
					{@const selected = normalizedValues.includes(option.value.toLowerCase())}
					<button
						type="button"
						class="option"
						class:selected
						role="option"
						aria-selected={selected}
						onclick={() => toggle(option.value)}
					>
						<span class="check" class:on={selected} aria-hidden="true">
							{#if selected}<Check size={12} strokeWidth={2.2} />{/if}
						</span>
						<span class="option-text">
							<strong>{option.label}</strong>
							<small>{option.meta ?? option.value}</small>
						</span>
					</button>
				{:else}
					<div class="empty">{emptyLabel}</div>
				{/each}
			</div>
		</div>
	{/if}
</div>

<style>
	.multi-select {
		position: relative;
		display: block;
		width: 100%;
		min-width: 0;
	}

	.control {
		display: grid;
		grid-template-columns: minmax(0, 1fr) auto;
		align-items: start;
		gap: 0.4rem;
		min-height: 2.4rem;
		padding: 0.35rem 0.4rem 0.35rem 0.5rem;
		border: 1px solid var(--line-strong);
		border-radius: var(--radius-md);
		background: var(--surface);
	}

	.control.open,
	.control:focus-within {
		border-color: var(--ink);
		box-shadow: 0 0 0 1px var(--ink);
	}

	.multi-select.disabled {
		opacity: 0.55;
		pointer-events: none;
	}

	.chips-area {
		min-width: 0;
		padding: 0.08rem 0;
	}

	.placeholder {
		display: block;
		padding: 0.2rem 0;
		color: var(--ink-faint);
		font-size: 0.75rem;
	}

	.chips {
		display: flex;
		flex-wrap: wrap;
		gap: 0.28rem;
	}

	.chip {
		display: inline-flex;
		align-items: center;
		gap: 0.1rem;
		max-width: 100%;
		min-height: 1.45rem;
		padding: 0.08rem 0.15rem 0.08rem 0.4rem;
		border: 1px solid var(--line);
		border-radius: var(--radius-sm);
		background: var(--surface-subtle);
	}

	.chip-label {
		overflow: hidden;
		max-width: 8.5rem;
		font-size: 0.68rem;
		font-weight: 620;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.actions {
		display: inline-flex;
		align-items: center;
		gap: 0.15rem;
		padding-top: 0.08rem;
		color: var(--ink-muted);
	}

	.icon-btn,
	.chip-remove {
		display: grid;
		place-items: center;
		width: 1.35rem;
		height: 1.35rem;
		padding: 0;
		border: 0;
		border-radius: var(--radius-sm);
		background: transparent;
		color: var(--ink-muted);
		cursor: pointer;
	}

	.chip-remove {
		width: 1.15rem;
		height: 1.15rem;
	}

	.icon-btn:hover,
	.chip-remove:hover {
		color: var(--ink);
		background: var(--surface-hover);
	}

	.count {
		min-width: 1.15rem;
		padding: 0.05rem 0.28rem;
		border: 1px solid var(--line);
		border-radius: var(--radius-sm);
		color: var(--ink-muted);
		font-family: var(--font-mono);
		font-size: 0.58rem;
		font-weight: 700;
		text-align: center;
	}

	.count.empty {
		opacity: 0.55;
	}

	.panel {
		position: absolute;
		top: calc(100% + 0.3rem);
		right: 0;
		left: 0;
		z-index: 40;
		display: flex;
		flex-direction: column;
		overflow: hidden;
		border: 1px solid var(--line-strong);
		border-radius: var(--radius-md);
		background: var(--surface);
		box-shadow: 0 8px 24px rgb(0 0 0 / 8%);
	}

	.search {
		display: grid;
		grid-template-columns: auto minmax(0, 1fr);
		align-items: center;
		gap: 0.4rem;
		padding: 0.45rem 0.55rem;
		border-bottom: 1px solid var(--line);
		color: var(--ink-faint);
	}

	.search input {
		width: 100%;
		min-height: 1.7rem;
		padding: 0;
		border: 0;
		background: transparent;
		color: var(--ink);
		font-size: 0.74rem;
		outline: none;
	}

	.options {
		display: flex;
		flex-direction: column;
		max-height: 15rem;
		overflow-y: auto;
	}

	.option {
		display: grid;
		grid-template-columns: 1.1rem minmax(0, 1fr);
		align-items: center;
		gap: 0.5rem;
		width: 100%;
		min-height: 2.45rem;
		padding: 0.4rem 0.6rem;
		border: 0;
		border-bottom: 1px solid var(--line);
		background: var(--surface);
		color: var(--ink);
		text-align: left;
		cursor: pointer;
	}

	.option:last-child {
		border-bottom: 0;
	}

	.option:hover,
	.option.selected {
		background: var(--surface-subtle);
	}

	.check {
		display: grid;
		place-items: center;
		width: 1.05rem;
		height: 1.05rem;
		border: 1px solid var(--line-strong);
		border-radius: var(--radius-sm);
		background: var(--surface);
		color: var(--surface);
	}

	.check.on {
		border-color: var(--ink);
		background: var(--ink);
		color: var(--surface);
	}

	.option-text {
		min-width: 0;
	}

	.option-text strong,
	.option-text small {
		display: block;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.option-text strong {
		font-size: 0.72rem;
		font-weight: 650;
	}

	.option-text small {
		margin-top: 0.08rem;
		color: var(--ink-faint);
		font-family: var(--font-mono);
		font-size: 0.56rem;
	}

	.empty {
		padding: 1rem 0.75rem;
		color: var(--ink-faint);
		font-size: 0.7rem;
		text-align: center;
	}
</style>
