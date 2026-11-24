<script lang="ts">
  // EPUB, MOBI, AZW3 y FB2 con foliate-js (src/vendor/foliate-js). Arriba: volver, titulo, el
  // capitulo actual, indice y ajustes; abajo: anterior / barra con el % / siguiente. Tocar los
  // costados pasa de pagina, el centro muestra u oculta las barras. Los ajustes de lectura
  // (BookSettings) se aplican con estilos dentro del libro.
  import { ArrowLeft, ChevronLeft, ChevronRight, List, Settings2 } from "@lucide/svelte";
  import interUrl from "@fontsource-variable/inter/files/inter-latin-wght-normal.woff2?url";
  import literataItalicUrl from "@fontsource-variable/literata/files/literata-latin-wght-italic.woff2?url";
  import literataUrl from "@fontsource-variable/literata/files/literata-latin-wght-normal.woff2?url";
  import { onDestroy, onMount } from "svelte";
  import BookSettings from "./BookSettings.svelte";
  import Drawer from "./Drawer.svelte";
  import { num, t } from "../lib/i18n.svelte";
  import { barBottom, barTop } from "../lib/motion";
  import { bookPrefs, PAGE_COLORS } from "../lib/readerPrefs.svelte";
  import { scale } from "../lib/scale.svelte";
  import type { Book } from "../lib/types";

  type TocItem = { label: string; href: string; subitems?: TocItem[] };
  type FoliateView = HTMLElement & {
    open: (f: File) => Promise<void>;
    init: (o: { lastLocation?: string | null; showTextStart?: boolean }) => Promise<void>;
    goTo: (href: string) => Promise<void>;
    goToFraction: (f: number) => Promise<void>;
    goLeft: () => Promise<void>;
    goRight: () => Promise<void>;
    prev: () => Promise<void>;
    next: () => Promise<void>;
    close: () => void;
    book: { toc?: TocItem[]; dir?: string; metadata?: { title?: unknown } };
    renderer: HTMLElement & { setStyles?: (css: string) => void };
  };

  let {
    book,
    url,
    start = "",
    onback,
    ontitle,
    onprogress,
  }: { book: Book; url: string; start?: string; onback: () => void; ontitle?: () => void; onprogress: (cfi: string, fraction: number) => void } = $props();

  let host = $state<HTMLDivElement>();
  let view: FoliateView | null = null;
  let chrome = $state(true);
  let settings = $state(false);
  let tocOpen = $state(false);
  let toc = $state<{ label: string; href: string; depth: number }[]>([]);
  let fraction = $state(0);
  let section = $state("");
  let currentHref = $state("");
  let error = $state("");
  let loading = $state(true);
  let scrub = $state<number | null>(null);

  const colors = $derived(PAGE_COLORS[bookPrefs.color]);

  function css(): string {
    const abs = (u: string) => new URL(u, location.href).href;
    const family =
      bookPrefs.font === "literata" ? `"LibLiterata", Georgia, serif` : bookPrefs.font === "inter" ? `"LibInter", system-ui, sans-serif` : "";
    return `
      @namespace epub "http://www.idpf.org/2007/ops";
      @font-face { font-family: "LibLiterata"; src: url("${abs(literataUrl)}") format("woff2"); font-weight: 200 900; font-style: normal; }
      @font-face { font-family: "LibLiterata"; src: url("${abs(literataItalicUrl)}") format("woff2"); font-weight: 200 900; font-style: italic; }
      @font-face { font-family: "LibInter"; src: url("${abs(interUrl)}") format("woff2"); font-weight: 100 900; }
      html { color-scheme: ${bookPrefs.color === "white" || bookPrefs.color === "sepia" ? "light" : "dark"};
             color: ${colors.fg} !important; background: ${colors.bg} !important; font-size: ${Math.round(bookPrefs.size * scale.factor)}% !important; }
      body { color: inherit !important; background: transparent !important; ${family ? `font-family: ${family} !important;` : ""} }
      ${family ? `p, li, blockquote, dd, div, span { font-family: inherit !important; }` : ""}
      p, li, blockquote, dd { line-height: ${bookPrefs.lineHeight} !important; text-align: ${bookPrefs.justify ? "justify" : "start"};
             hyphens: auto; -webkit-hyphens: auto; widows: 2; orphans: 2; }
      a:link, a:visited { color: ${bookPrefs.color === "white" || bookPrefs.color === "sepia" ? "#2a6db0" : "#8cc4ff"}; }
      img, svg { max-width: 100%; height: auto; }
      pre { white-space: pre-wrap !important; }
      aside[epub|type~="footnote"], aside[epub|type~="endnote"] { display: none; }
    `;
  }

  function applyStyles() {
    if (!view?.renderer) return;
    const r = view.renderer;
    r.setAttribute("flow", bookPrefs.flow);
    // foliate toma el espacio de los costados como % del ancho: los px elegidos se pasan a %.
    r.setAttribute("gap", `${Math.max(2, Math.min(30, (bookPrefs.margin / Math.max(320, innerWidth)) * 100 * 2)).toFixed(2)}%`);
    // El lector va sin zoom (foliate mide mucho con rects): el texto y la columna se agrandan aca
    // con el mismo factor que el resto de la interfaz, para que en 2K no quede chico.
    r.setAttribute("margin", `${Math.round(56 * scale.factor)}px`);
    r.setAttribute("max-inline-size", `${Math.round(760 * scale.factor)}px`);
    r.setAttribute("max-column-count", innerWidth >= 1200 * scale.factor ? "2" : "1");
    r.setStyles?.(css());
  }

  // Cada cambio de ajustes se aplica en vivo.
  $effect(() => {
    void [bookPrefs.font, bookPrefs.size, bookPrefs.lineHeight, bookPrefs.margin, bookPrefs.justify, bookPrefs.flow, bookPrefs.color, scale.factor];
    applyStyles();
  });

  function flatten(items: TocItem[] | undefined, depth = 0, out: { label: string; href: string; depth: number }[] = []) {
    for (const it of items ?? []) {
      out.push({ label: (it.label ?? "").trim(), href: it.href, depth });
      flatten(it.subitems, depth + 1, out);
    }
    return out;
  }

  // Toques dentro del libro (el iframe se traga los eventos: se escuchan adentro).
  function onDocClick(e: MouseEvent, doc: Document) {
    const target = e.target as Element | null;
    if (target?.closest?.("a") || doc.getSelection()?.toString()) return;
    const frame = doc.defaultView?.frameElement as HTMLElement | null;
    const left = (frame?.getBoundingClientRect().left ?? 0) + e.clientX;
    const x = left / innerWidth;
    if (bookPrefs.flow === "scrolled" || (x > 0.3 && x < 0.7)) {
      chrome = !chrome;
      return;
    }
    if (x <= 0.3) view?.goLeft();
    else view?.goRight();
    chrome = false;
  }

  function onKey(e: KeyboardEvent) {
    if (settings || tocOpen || e.ctrlKey || e.metaKey || e.altKey) return;
    if (e.key === "ArrowLeft" || e.key === "PageUp") (e.preventDefault(), view?.goLeft());
    else if (e.key === "ArrowRight" || e.key === "PageDown" || e.key === " ") (e.preventDefault(), view?.goRight());
    else if (e.key === "Escape") (e.preventDefault(), onback());
  }

  onMount(() => {
    let dead = false;
    (async () => {
      try {
        await import("../vendor/foliate-js/view.js");
        const res = await fetch(url, { credentials: "same-origin" });
        if (!res.ok) throw new Error(t("common.errorStatus", { status: res.status }));
        const blob = await res.blob();
        if (dead) return;
        const file = new File([blob], `book.${book.format}`, { type: blob.type });
        const v = document.createElement("foliate-view") as FoliateView;
        view = v;
        host!.append(v);
        v.addEventListener("load", ((e: CustomEvent<{ doc: Document }>) => {
          const doc = e.detail.doc;
          doc.addEventListener("click", (ev) => onDocClick(ev, doc));
          doc.addEventListener("keydown", onKey);
        }) as EventListener);
        v.addEventListener("relocate", ((e: CustomEvent<{ fraction: number; cfi: string; tocItem?: { label?: string; href?: string } }>) => {
          const d = e.detail;
          fraction = d.fraction ?? 0;
          section = (d.tocItem?.label ?? "").trim();
          currentHref = d.tocItem?.href ?? "";
          if (d.cfi) onprogress(d.cfi, fraction);
        }) as EventListener);
        await v.open(file);
        toc = flatten(v.book.toc);
        applyStyles();
        await v.init(start ? { lastLocation: start } : { showTextStart: true });
        loading = false;
        idleHide();
      } catch (e) {
        error = e instanceof Error ? e.message : String(e);
        loading = false;
      }
    })();
    return () => {
      dead = true;
    };
  });

  onDestroy(() => {
    try {
      view?.close();
    } catch {}
    view?.remove();
  });

  let idle = 0;
  function idleHide() {
    clearTimeout(idle);
    idle = window.setTimeout(() => !settings && !tocOpen && (chrome = false), 3000);
  }

  const pct = $derived(`${num(Math.round((scrub ?? fraction) * 1000) / 10, { maximumFractionDigits: 1 })} %`);
</script>

<svelte:window onkeydown={onKey} />

<div class="ebook" style="--page-bg: {colors.bg}; --page-fg: {colors.fg}">
  <div class="host" bind:this={host}></div>

  {#if loading}
    <div class="center"><div class="pulse"></div></div>
  {:else if error}
    <div class="center err">
      <h3>{t("reader.bookFailed")}</h3>
      <p>{error}</p>
      <button class="btn" onclick={onback}>{t("common.back")}</button>
    </div>
  {/if}

  {#if chrome || loading}
    <header class="bar top" transition:barTop>
      <button class="icon-btn" onclick={onback} aria-label={t("common.back")}><ArrowLeft size={20} /></button>
      <div class="titles">
        {#if ontitle}
          <button class="t1 ellipsis link" onclick={ontitle} title={t("book.details")}>{book.title}</button>
        {:else}
          <span class="t1 ellipsis">{book.title}</span>
        {/if}
        {#if section}<span class="t2 ellipsis">{section}</span>{:else if book.author}<span class="t2 ellipsis">{book.author}</span>{/if}
      </div>
      {#if toc.length}<button class="icon-btn" onclick={() => (tocOpen = true)} title={t("book.toc")}><List size={19} /></button>{/if}
      <button class="icon-btn" onclick={() => (settings = true)} title={t("reader.settings")}><Settings2 size={19} /></button>
    </header>
    {#if !loading}
      <footer class="bar bottom" transition:barBottom>
        <button class="icon-btn" onclick={() => view?.goLeft()} aria-label={t("reader.prevPage")}><ChevronLeft size={20} /></button>
        <input
          class="scrub"
          type="range"
          min="0"
          max="1"
          step="0.001"
          value={scrub ?? fraction}
          oninput={(e) => (scrub = Number(e.currentTarget.value))}
          onchange={(e) => {
            const f = Number(e.currentTarget.value);
            scrub = null;
            view?.goToFraction(f);
          }}
          aria-label={t("reader.page")}
        />
        <span class="count mono">{pct}</span>
        <button class="icon-btn" onclick={() => view?.goRight()} aria-label={t("reader.nextPage")}><ChevronRight size={20} /></button>
      </footer>
    {/if}
  {/if}
</div>

{#if settings}<BookSettings onclose={() => (settings = false)} />{/if}

{#if tocOpen}
  <Drawer title={t("book.toc")} media onclose={() => (tocOpen = false)}>
    <div class="toc">
      {#each toc as it, i (i)}
        <button
          class="toc-it"
          class:on={it.href === currentHref}
          style="padding-left: {12 + it.depth * 16}px"
          onclick={() => {
            tocOpen = false;
            view?.goTo(it.href);
          }}>{it.label}</button
        >
      {/each}
    </div>
  </Drawer>
{/if}

<style>
  .ebook {
    position: fixed;
    inset: 0;
    z-index: 20;
    background: var(--page-bg);
    color: var(--page-fg);
  }
  .host {
    position: absolute;
    inset: 0;
  }
  .host :global(foliate-view) {
    display: block;
    width: 100%;
    height: 100%;
  }
  .center {
    position: absolute;
    inset: 0;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 10px;
    text-align: center;
    padding: 20px;
  }
  .err p {
    margin: 0;
    opacity: 0.7;
  }
  .pulse {
    width: 46px;
    height: 64px;
    border-radius: 6px;
    background: color-mix(in srgb, var(--accent) 70%, transparent);
    animation: breathe 1.2s ease-in-out infinite;
  }
  @keyframes breathe {
    50% {
      transform: scale(0.88);
      opacity: 0.5;
    }
  }
  /* Sin zoom en el lector; las barras si se agrandan con "Tamaño de la interfaz" (lib/scale.svelte.ts). */
  .bar {
    zoom: var(--ui-zoom, 1);
    position: absolute;
    left: 0;
    right: 0;
    z-index: 5;
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 8px 10px;
    color: var(--on-media);
    background: color-mix(in srgb, #0b0b0d 82%, transparent);
    backdrop-filter: blur(16px) saturate(140%);
  }
  .bar.top {
    top: 0;
    padding-top: calc(8px + env(safe-area-inset-top));
    border-bottom: 1px solid var(--media-line);
  }
  .bar.bottom {
    bottom: 0;
    padding-bottom: calc(10px + env(safe-area-inset-bottom));
    border-top: 1px solid var(--media-line);
  }
  .bar .icon-btn {
    width: 40px;
    height: 40px;
    color: var(--on-media);
    flex: none;
  }
  .bar .icon-btn:hover {
    background: rgba(255, 255, 255, 0.1);
  }
  .titles {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    line-height: 1.25;
  }
  .t1 {
    font-weight: 650;
    font-size: 14px;
  }
  .link {
    text-align: left;
    text-decoration: underline;
    text-decoration-color: transparent;
    text-underline-offset: 3px;
    transition: text-decoration-color 0.15s;
  }
  .link:hover {
    text-decoration-color: currentColor;
  }
  .t2 {
    font-size: 12px;
    color: var(--on-media-2);
  }
  .scrub {
    flex: 1;
    height: 28px;
    padding: 0;
    border: 0;
    background: none;
    accent-color: var(--accent);
  }
  .count {
    min-width: 58px;
    text-align: center;
    font-size: 12.5px;
    color: var(--on-media-2);
  }
  .toc {
    display: flex;
    flex-direction: column;
    gap: 1px;
    margin: -6px -8px;
  }
  .toc-it {
    text-align: left;
    padding: 9px 12px;
    border-radius: var(--radius-sm);
    line-height: 1.35;
  }
  .toc-it:hover {
    background: var(--hover);
  }
  .toc-it.on {
    background: color-mix(in srgb, var(--accent) 18%, transparent);
    box-shadow: inset 3px 0 0 var(--accent);
  }
</style>
