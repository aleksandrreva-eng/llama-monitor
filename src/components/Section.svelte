<script>
  /**
   * Shared widget section: the padded block with a 1px top separator that every
   * telemetry component used to re-declare in its own `<style>` block.
   */
  export let title = "";
</script>

<div class="section">
  {#if title}
    <div class="section-title">{title}</div>
  {/if}
  <slot />
</div>

<style>
  .section {
    padding: 10px 12px 12px;
    border-top: 1px solid var(--divider);
    /* Anchor for a section's own tooltip/overlay. */
    position: relative;
  }

  .section-title {
    font-size: 10px;
    font-weight: 600;
    letter-spacing: 0.6px;
    text-transform: uppercase;
    color: var(--text-title);
    margin-bottom: 8px;
  }
</style>
