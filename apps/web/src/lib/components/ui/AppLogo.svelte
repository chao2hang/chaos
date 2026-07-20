<script lang="ts">
	let { compact = false } = $props<{ compact?: boolean }>();
</script>

<span class="logo" class:compact aria-label="chaos">
	<span class="mark" aria-hidden="true">
		<span></span>
		<span></span>
	</span>
	{#if !compact}<span class="wordmark">chaos</span>{/if}
</span>

<style>
	.logo {
		display: inline-flex;
		align-items: center;
		gap: 0.65rem;
		min-width: 0;
		color: var(--ink);
	}

	.mark {
		display: grid;
		grid-template-columns: repeat(2, 0.35rem);
		gap: 0.16rem;
		width: 0.86rem;
		height: 0.86rem;
		transform: rotate(45deg);
		flex: 0 0 auto;
	}

	.mark span {
		display: block;
		background: currentColor;
	}

	.mark span:last-child {
		margin-top: 0.24rem;
	}

	.wordmark {
		font-size: 1rem;
		font-weight: 720;
		line-height: 1;
	}

	.compact {
		width: 1rem;
	}
</style>
