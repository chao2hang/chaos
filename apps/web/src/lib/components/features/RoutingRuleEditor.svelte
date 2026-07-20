<script lang="ts">
	import { ArrowDown, ArrowUp, Copy, Plus, Route, Trash2 } from '@lucide/svelte';
	import type { RoutingRuleDto } from '$lib/api';
	import { t } from '$lib/i18n.svelte';
	import Button from '$lib/components/ui/Button.svelte';
	import EmptyState from '$lib/components/ui/EmptyState.svelte';
	import Toggle from '$lib/components/ui/Toggle.svelte';

	let {
		rules = $bindable(),
		outbounds,
		busy = false,
		onchange
	}: {
		rules: RoutingRuleDto[];
		outbounds: string[];
		busy?: boolean;
		onchange?: () => void;
	} = $props();

	function emit(next: RoutingRuleDto[]) {
		rules = next;
		onchange?.();
	}

	function updateRule(index: number, patch: Partial<RoutingRuleDto>) {
		emit(rules.map((rule, ruleIndex) => (ruleIndex === index ? { ...rule, ...patch } : rule)));
	}

	function addRule() {
		emit([
			...rules,
			{
				expression: '',
				outbound: outbounds.includes('proxy') ? 'proxy' : (outbounds[0] ?? ''),
				enabled: true
			}
		]);
	}

	function duplicateRule(index: number) {
		const source = rules[index];
		const next = [...rules];
		next.splice(index + 1, 0, { ...source });
		emit(next);
	}

	function removeRule(index: number) {
		emit(rules.filter((_, ruleIndex) => ruleIndex !== index));
	}

	function moveRule(index: number, direction: -1 | 1) {
		const target = index + direction;
		if (target < 0 || target >= rules.length) return;
		const next = [...rules];
		[next[index], next[target]] = [next[target], next[index]];
		emit(next);
	}

	function optionsFor(value: string) {
		return value && !outbounds.includes(value) ? [value, ...outbounds] : outbounds;
	}
</script>

{#if rules.length}
	<ol class="rule-list">
		{#each rules as rule, index (index)}
			<li class:disabled={!rule.enabled}>
				<header>
					<div class="rule-index">
						<span>{String(index + 1).padStart(2, '0')}</span>
						<Toggle
							checked={rule.enabled}
							label={t('common.enabled')}
							disabled={busy}
							onchange={(checked) => updateRule(index, { enabled: checked })}
						/>
					</div>
					<div class="rule-actions">
						<Button
							variant="ghost"
							size="icon"
							icon={ArrowUp}
							disabled={busy || index === 0}
							aria-label={t('flow.moveUp')}
							title={t('flow.moveUp')}
							onclick={() => moveRule(index, -1)}
						/>
						<Button
							variant="ghost"
							size="icon"
							icon={ArrowDown}
							disabled={busy || index === rules.length - 1}
							aria-label={t('flow.moveDown')}
							title={t('flow.moveDown')}
							onclick={() => moveRule(index, 1)}
						/>
						<Button
							variant="ghost"
							size="icon"
							icon={Copy}
							disabled={busy}
							aria-label={t('routing.duplicateRule')}
							title={t('routing.duplicateRule')}
							onclick={() => duplicateRule(index)}
						/>
						<Button
							variant="ghost"
							size="icon"
							icon={Trash2}
							disabled={busy}
							aria-label={t('routing.deleteRule', { index: index + 1 })}
							title={t('common.delete')}
							onclick={() => removeRule(index)}
						/>
					</div>
				</header>

				<div class="rule-fields">
					<label>
						<span>{t('routing.expression')}</span>
						<input
							type="text"
							value={rule.expression}
							class:invalid={rule.enabled && !rule.expression.trim()}
							placeholder="domain(example.com)"
							disabled={busy}
							oninput={(event) =>
								updateRule(index, { expression: (event.currentTarget as HTMLInputElement).value })}
						/>
					</label>
					<label>
						<span>{t('routing.outbound')}</span>
						<select
							value={rule.outbound}
							class:invalid={rule.enabled && !rule.outbound.trim()}
							disabled={busy}
							onchange={(event) =>
								updateRule(index, { outbound: (event.currentTarget as HTMLSelectElement).value })}
						>
							{#each optionsFor(rule.outbound) as outbound (outbound)}
								<option value={outbound}>{outbound}</option>
							{/each}
						</select>
					</label>
				</div>
			</li>
		{/each}
	</ol>
{:else}
	<EmptyState icon={Route} title={t('routing.empty')} description={t('routing.emptyDescription')}>
		{#snippet actions()}<Button icon={Plus} onclick={addRule}>{t('routing.addRule')}</Button>{/snippet}
	</EmptyState>
{/if}

{#if rules.length}
	<div class="add-row">
		<Button icon={Plus} disabled={busy} onclick={addRule}>{t('routing.addRule')}</Button>
	</div>
{/if}

<style>
	.rule-list {
		display: flex;
		flex-direction: column;
		gap: var(--space-3);
		margin: 0;
		padding: 0;
		list-style: none;
	}

	.rule-list > li {
		border: 1px solid var(--line);
		border-radius: var(--radius-md);
		background: var(--surface);
		transition: opacity 120ms ease;
	}

	.rule-list > li.disabled {
		opacity: 0.58;
	}

	header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: var(--space-3);
		min-height: 2.8rem;
		padding: 0.35rem 0.5rem 0.35rem var(--space-3);
		border-bottom: 1px solid var(--line);
		background: var(--surface-subtle);
	}

	.rule-index,
	.rule-actions {
		display: flex;
		align-items: center;
		gap: var(--space-2);
	}

	.rule-index > span {
		width: 1.8rem;
		color: var(--ink-muted);
		font-family: var(--font-mono);
		font-size: 0.7rem;
		font-weight: 650;
	}

	.rule-fields {
		display: grid;
		grid-template-columns: minmax(0, 2fr) minmax(10rem, 1fr);
		gap: var(--space-3);
		padding: var(--space-3);
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

	input {
		font-family: var(--font-mono);
		font-size: 0.76rem;
	}

	.invalid {
		border-color: var(--ink);
		background: var(--danger-surface);
	}

	.add-row {
		padding-top: var(--space-3);
	}

	@media (max-width: 620px) {
		.rule-fields {
			grid-template-columns: 1fr;
		}

		header {
			align-items: flex-start;
			flex-direction: column;
		}

		.rule-actions {
			align-self: stretch;
			justify-content: flex-end;
		}
	}
</style>
