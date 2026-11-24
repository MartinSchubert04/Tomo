<script lang="ts">
  // Libros .txt: texto corrido con los ajustes de lectura (letra, tamaño, interlineado, margenes,
  // color). Se scrollea; el progreso es la fraccion del scroll.
  import { ArrowLeft, Settings2 } from "@lucide/svelte";
  import { onMount } from "svelte";
  import BookSettings from "./BookSettings.svelte";
  import { num, t } from "../lib/i18n.svelte";
  import { barTop } from "../lib/motion";
  import { bookPrefs, PAGE_COLORS } from "../lib/readerPrefs.svelte";
  import type { Book } from "../lib/types";

  let {
    book,
    url,
    start = 0,
    onback,
    onprogress,
  }: { book: Book; url: string; start?: number; onback: () => void; onprogress: (fraction: number) => void } = $props();

  let text = $state<string | null>(null);
  let error = $state("");
  let box = $state<HTMLDivElement>();
  let settings = $state(false);
  let chrome = $state(true);
  let frac = $state(0);
  let lastY = 0;

  const colors = $derived(PAGE_COLORS[bookPrefs.color]);
  const family = $derived(
    bookPrefs.font === "inter" ? `"Inter Variable", system-ui, sans-serif` : `"Literata Variable", Georgia, serif`,
  );

  onMount(async () => {
    try {
      const r = await fetch(url, { credentials: "same-origin" });
      if (!r.ok) throw new Error(t("common.errorStatus", { status: r.status }));
      text = await r.text();
      requestAnimationFrame(() => {
        if (box) box.scrollTop = start * (box.scrollHeight - box.clientHeight);
      });
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    }
  });

  function onScroll() {
    if (!box) return;
    const max = box.scrollHeight - box.clientHeight;
    frac = max > 0 ? box.scrollTop / max : 1;
    onprogress(frac);
    const y = box.scrollTop;
    if (y > lastY + 12) chrome = false;
    else if (y < lastY - 40) chrome = true;
    lastY = y;
  }
</script>

<div class="txt" style="--page-bg: {colors.bg}; --page-fg: {colors.fg}">
  <div class="scroll" bind:this={box} onscroll={onScroll} onclick={() => (chrome = !chrome)} role="presentation">
    {#if text !== null}
      <article
        style="font-family: {family}; font-size: {bookPrefs.size * 0.17}px; line-height: {bookPrefs.lineHeight}; padding: 80px {bookPrefs.margin}px 100px; text-align: {bookPrefs.justify ? 'justify' : 'start'}"
      >
        {text}
      </article>
    {:else if error}
      <p class="err">{error}</p>
    {/if}
  </div>
  {#if chrome}
    <header class="bar" transition:barTop>
      <button class="icon-btn" onclick={onback} aria-label={t("common.back")}><ArrowLeft size={20} /></button>
      <span class="t1 ellipsis">{book.title}</span>
      <span class="mono pct">{num(Math.round(frac * 100))} %</span>
      <button class="icon-btn" onclick={() => (settings = true)} title={t("reader.settings")}><Settings2 size={19} /></button>
    </header>
  {/if}
</div>

{#if settings}<BookSettings flow={false} onclose={() => (settings = false)} />{/if}

<style>
  .txt {
    position: fixed;
    inset: 0;
    z-index: 20;
    background: var(--page-bg);
    color: var(--page-fg);
  }
  .scroll {
    position: absolute;
    inset: 0;
    overflow-y: auto;
  }
  article {
    max-width: 760px;
    margin: 0 auto;
    white-space: pre-wrap;
    word-wrap: break-word;
    hyphens: auto;
    user-select: text;
  }
  .err {
    padding: 100px 20px;
    text-align: center;
  }
  .bar {
    position: absolute;
    left: 0;
    right: 0;
    top: 0;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 10px;
    padding-top: calc(8px + env(safe-area-inset-top));
    color: var(--on-media);
    background: color-mix(in srgb, #0b0b0d 82%, transparent);
    backdrop-filter: blur(16px);
    border-bottom: 1px solid var(--media-line);
  }
  .bar .icon-btn {
    width: 40px;
    height: 40px;
    color: var(--on-media);
  }
  .t1 {
    flex: 1;
    font-weight: 650;
  }
  .pct {
    font-size: 12.5px;
    color: var(--on-media-2);
  }
</style>
