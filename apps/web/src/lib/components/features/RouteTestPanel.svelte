<script lang="ts">
	import { FlaskConical, Play, Route, X } from '@lucide/svelte';
	import { tick } from 'svelte';
	import {
		ApiClientError,
		simulateOrchestration,
		type OrchestrationDocument,
		type RouteSimulation
	} from '$lib/api';
	import { GEOIP_OPTIONS, GEOSITE_OPTIONS } from '$lib/geoOptions';
	import { apiErrorText, t } from '$lib/i18n.svelte';
	import Button from '$lib/components/ui/Button.svelte';
	import Field from '$lib/components/ui/Field.svelte';
	import MultiSelect from '$lib/components/ui/MultiSelect.svelte';

	let {
		open = $bindable(false),
		document: flowDocument,
		disabled = false
	}: {
		open?: boolean;
		document: OrchestrationDocument;
		disabled?: boolean;
	} = $props();

	let domain = $state('');
	let destIp = $state('');
	let geoip = $state<string[]>([]);
	let geosite = $state<string[]>([]);
	let busy = $state(false);
	let error = $state('');
	let result = $state<RouteSimulation | null>(null);
	let dialogElement = $state<HTMLDivElement | null>(null);

	const geoipSelectOptions = $derived(
		GEOIP_OPTIONS.map((opt) => ({
			value: opt.code,
			label: t(opt.labelKey),
			meta: opt.code
		}))
	);
	const geositeSelectOptions = $derived(
		GEOSITE_OPTIONS.map((opt) => ({
			value: opt.code,
			label: t(opt.labelKey),
			meta: opt.code
		}))
	);

	const canRun = $derived(
		!disabled &&
			!busy &&
			(domain.trim() !== '' || destIp.trim() !== '' || geoip.length > 0 || geosite.length > 0)
	);

	function reasonLabel(reason: string): string {
		const key = `flow.test.reason.${reason}`;
		const label = t(key);
		return label === key ? reason : label;
	}

	function matcherLabel(kind: string | undefined): string {
		if (!kind) return '—';
		const map: Record<string, string> = {
			domain_suffix: t('flow.matcher.domainSuffix'),
			destination_cidr: t('flow.matcher.destinationCidr'),
			geosite: t('flow.matcher.geosite'),
			geoip: t('flow.matcher.geoip')
		};
		return map[kind] ?? kind;
	}

	function close() {
		if (busy) return;
		open = false;
	}

	function onKeydown(event: KeyboardEvent) {
		if (event.key === 'Escape') close();
		if (event.key !== 'Tab' || !dialogElement) return;
		const focusable = Array.from(
			dialogElement.querySelectorAll<HTMLElement>(
				'button:not(:disabled), [href], input:not(:disabled), select:not(:disabled), textarea:not(:disabled), [tabindex]:not([tabindex="-1"])'
			)
		);
		if (!focusable.length) return;
		const first = focusable[0];
		const last = focusable.at(-1)!;
		const active = window.document.activeElement;
		if (event.shiftKey && active === first) {
			event.preventDefault();
			last.focus();
		} else if (!event.shiftKey && active === last) {
			event.preventDefault();
			first.focus();
		}
	}

	$effect(() => {
		if (!open || typeof window === 'undefined') return;
		const pageDocument = window.document;
		const previous = pageDocument.activeElement as HTMLElement | null;
		const previousOverflow = pageDocument.body.style.overflow;
		pageDocument.body.style.overflow = 'hidden';
		error = '';
		result = null;
		void tick().then(() =>
			dialogElement?.querySelector<HTMLElement>('input:not(:disabled), button:not(:disabled)')?.focus()
		);
		return () => {
			pageDocument.body.style.overflow = previousOverflow;
			previous?.focus();
		};
	});

	async function run() {
		if (!canRun) return;
		busy = true;
		error = '';
		result = null;
		try {
			result = await simulateOrchestration(flowDocument, {
				domain: domain.trim() || null,
				dest_ip: destIp.trim() || null,
				geoip: geoip.length ? geoip : [],
				geosite: geosite.length ? geosite : []
			});
		} catch (cause) {
			error =
				cause instanceof ApiClientError
					? apiErrorText(cause)
					: cause instanceof Error
						? cause.message
						: t('flow.test.failed');
		} finally {
			busy = false;
		}
	}
</script>

<svelte:window onkeydown={open ? onKeydown : undefined} />

{#if open}
	<div class="dialog-layer" role="presentation">
		<button class="backdrop" type="button" aria-label={t('common.cancel')} onclick={close}></button>
		<div
			bind:this={dialogElement}
			class="dialog"
			role="dialog"
			aria-modal="true"
			aria-labelledby="route-test-title"
			aria-describedby="route-test-desc"
			data-testid="route-test-dialog"
		>
			<header>
				<span class="dialog-icon"><FlaskConical size={17} strokeWidth={1.8} aria-hidden="true" /></span>
				<div>
					<span class="badge">{t('flow.test.badge')}</span>
					<h2 id="route-test-title">{t('flow.test.title')}</h2>
					<p id="route-test-desc">{t('flow.test.description')}</p>
				</div>
				<button
					class="close"
					type="button"
					aria-label={t('common.cancel')}
					title={t('common.cancel')}
					disabled={busy}
					onclick={close}
				>
					<X size={16} strokeWidth={1.8} aria-hidden="true" />
				</button>
			</header>

			<div class="dialog-body">
				<div class="fields">
					<Field label={t('flow.test.domain')} forId="route-test-domain" hint={t('flow.test.domainHint')}>
						<input
							id="route-test-domain"
							type="text"
							placeholder="api.fast.com"
							autocomplete="off"
							spellcheck="false"
							value={domain}
							disabled={disabled || busy}
							oninput={(event) => (domain = (event.currentTarget as HTMLInputElement).value)}
						/>
					</Field>
					<Field label={t('flow.test.destIp')} forId="route-test-ip" hint={t('flow.test.destIpHint')}>
						<input
							id="route-test-ip"
							type="text"
							placeholder="8.8.8.8"
							autocomplete="off"
							spellcheck="false"
							value={destIp}
							disabled={disabled || busy}
							oninput={(event) => (destIp = (event.currentTarget as HTMLInputElement).value)}
						/>
					</Field>
					<Field label={t('flow.test.geoip')} forId="route-test-geoip" hint={t('flow.test.geoipHint')}>
						<MultiSelect
							id="route-test-geoip"
							label={t('flow.test.geoip')}
							values={geoip}
							options={geoipSelectOptions}
							placeholder={t('flow.matcher.selectCodes')}
							searchPlaceholder={t('flow.matcher.searchCodes')}
							emptyLabel={t('common.noSearchResults')}
							disabled={disabled || busy}
							onChange={(next) => (geoip = next)}
						/>
					</Field>
					<Field label={t('flow.test.geosite')} forId="route-test-geosite" hint={t('flow.test.geositeHint')}>
						<MultiSelect
							id="route-test-geosite"
							label={t('flow.test.geosite')}
							values={geosite}
							options={geositeSelectOptions}
							placeholder={t('flow.matcher.selectCodes')}
							searchPlaceholder={t('flow.matcher.searchCodes')}
							emptyLabel={t('common.noSearchResults')}
							disabled={disabled || busy}
							onChange={(next) => (geosite = next)}
						/>
					</Field>
				</div>

				{#if error}
					<p class="error" role="alert">{error}</p>
				{/if}

				{#if result}
					<div class="result" data-testid="route-test-result">
						<div class="outcome" class:matched={result.matched}>
							<Route size={16} strokeWidth={1.8} aria-hidden="true" />
							<div>
								<span>{result.matched ? t('flow.test.matched') : t('flow.test.fallback')}</span>
								<strong>{result.outbound}</strong>
							</div>
						</div>
						{#if result.matched}
							<p class="match-detail">
								{matcherLabel(result.matched_matcher_kind)}
								{#if result.matched_pattern}
									<code>{result.matched_pattern}</code>
								{/if}
								{#if result.matched_condition}
									<small>{result.matched_condition}</small>
								{/if}
							</p>
						{/if}
						<ol class="trace">
							{#each result.steps as step, index (index)}
								<li class:hit={step.matched} class:skip={step.reason.startsWith('skip_')}>
									<span class="idx">{index + 1}</span>
									<div>
										<strong>
											{#if step.kind === 'fallback'}
												{t('flow.test.stepFallback')}
											{:else}
												{matcherLabel(step.matcher_kind)}
												{#if step.pattern}<code>{step.pattern}</code>{/if}
											{/if}
										</strong>
										<small>{reasonLabel(step.reason)} → {step.outbound}</small>
									</div>
								</li>
							{/each}
						</ol>
					</div>
				{:else if !error}
					<div class="empty">
						<FlaskConical size={16} strokeWidth={1.8} aria-hidden="true" />
						<span>{t('flow.test.empty')}</span>
					</div>
				{/if}
			</div>

			<footer>
				<Button onclick={close} disabled={busy}>{t('common.cancel')}</Button>
				<Button
					variant="primary"
					icon={Play}
					loading={busy}
					disabled={!canRun}
					data-testid="route-test-run"
					onclick={() => void run()}
				>
					{t('flow.test.run')}
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
		display: flex;
		width: min(36rem, 100%);
		max-height: min(90vh, 44rem);
		flex-direction: column;
		border: 1px solid var(--ink);
		border-radius: var(--radius-lg);
		background: var(--surface);
		box-shadow: var(--shadow-float);
		overflow: hidden;
	}

	header {
		display: grid;
		grid-template-columns: auto 1fr auto;
		gap: var(--space-3);
		padding: var(--space-5);
		border-bottom: 1px solid var(--line);
	}

	.dialog-icon {
		display: inline-grid;
		place-items: center;
		width: 2rem;
		height: 2rem;
		border: 1px solid var(--ink);
		border-radius: var(--radius-md);
		background: var(--surface-subtle, var(--surface));
	}

	.badge {
		display: block;
		color: var(--ink-faint);
		font-family: var(--font-mono);
		font-size: 0.57rem;
		font-weight: 700;
		text-transform: uppercase;
	}

	h2 {
		margin: 0.1rem 0 0;
		font-size: 0.95rem;
		font-weight: 700;
	}

	header p {
		margin: var(--space-1) 0 0;
		color: var(--ink-muted);
		font-size: 0.8rem;
		line-height: 1.4;
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

	.close:hover:not(:disabled) {
		background: var(--surface-subtle);
		color: var(--ink);
	}

	.close:disabled {
		opacity: 0.5;
	}

	.dialog-body {
		display: flex;
		min-height: 0;
		flex: 1 1 auto;
		flex-direction: column;
		gap: var(--space-3);
		padding: var(--space-5);
		overflow: auto;
	}

	.fields {
		display: grid;
		grid-template-columns: 1fr 1fr;
		gap: var(--space-3);
	}

	.fields :global(.field-wrap:nth-child(n + 3)) {
		grid-column: 1 / -1;
	}

	.fields :global(input[type='text']) {
		width: 100%;
		min-height: 2.15rem;
		padding: 0 0.65rem;
		border: 1px solid var(--line-strong);
		border-radius: var(--radius-sm);
		background: var(--surface-elevated, var(--surface));
		color: var(--ink);
		font-size: 0.8rem;
	}

	.error {
		margin: 0;
		color: var(--danger, #b42318);
		font-size: 0.78rem;
		line-height: 1.4;
	}

	.result {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
		padding: var(--space-3);
		border: 1px solid var(--line-strong);
		border-radius: var(--radius-md);
		background: var(--surface-muted, var(--surface));
	}

	.outcome {
		display: grid;
		grid-template-columns: auto minmax(0, 1fr);
		gap: var(--space-2);
		align-items: center;
	}

	.outcome span {
		display: block;
		color: var(--ink-muted);
		font-size: 0.68rem;
	}

	.outcome strong {
		display: block;
		font-family: var(--font-mono);
		font-size: 1rem;
	}

	.outcome.matched strong {
		text-decoration: underline;
		text-underline-offset: 0.15em;
	}

	.match-detail {
		display: flex;
		flex-wrap: wrap;
		align-items: baseline;
		gap: 0.35rem 0.55rem;
		margin: 0;
		color: var(--ink-muted);
		font-size: 0.72rem;
	}

	.match-detail code,
	.trace code {
		padding: 0.05rem 0.3rem;
		border: 1px solid var(--line);
		border-radius: var(--radius-sm);
		font-family: var(--font-mono);
		font-size: 0.68rem;
	}

	.match-detail small {
		width: 100%;
		font-family: var(--font-mono);
		font-size: 0.62rem;
		opacity: 0.85;
	}

	.trace {
		display: flex;
		margin: 0;
		padding: 0;
		flex-direction: column;
		gap: 0.35rem;
		list-style: none;
	}

	.trace li {
		display: grid;
		grid-template-columns: 1.4rem minmax(0, 1fr);
		gap: 0.4rem;
		align-items: start;
		padding: 0.4rem 0.45rem;
		border: 1px solid transparent;
		border-radius: var(--radius-sm);
		color: var(--ink-muted);
		font-size: 0.72rem;
	}

	.trace li.hit {
		border-color: var(--line-strong);
		background: var(--surface);
		color: var(--ink);
	}

	.trace li.skip {
		opacity: 0.72;
	}

	.trace .idx {
		font-family: var(--font-mono);
		font-size: 0.62rem;
		font-weight: 700;
	}

	.trace strong {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 0.3rem;
		font-size: 0.74rem;
	}

	.trace small {
		display: block;
		margin-top: 0.1rem;
		font-size: 0.66rem;
		line-height: 1.35;
	}

	.empty {
		display: flex;
		align-items: center;
		gap: var(--space-2);
		padding: var(--space-2) 0;
		color: var(--ink-faint);
		font-size: 0.75rem;
	}

	footer {
		display: flex;
		justify-content: flex-end;
		gap: var(--space-2);
		padding: var(--space-4) var(--space-5);
		border-top: 1px solid var(--line);
		background: var(--surface);
	}

	@media (max-width: 640px) {
		.fields {
			grid-template-columns: 1fr;
		}

		.fields :global(.field-wrap:nth-child(n + 3)) {
			grid-column: auto;
		}
	}
</style>
