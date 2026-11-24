<script lang="ts">
  // Panel para formularios y ajustes. En la PC entra desde la derecha; en el celular es una hoja
  // que sube desde abajo (se cierra tocando afuera o arrastrandola hacia abajo). Portado de Ethereon.
  import { X } from "@lucide/svelte";
  import type { Snippet } from "svelte";
  import { toCss } from "../lib/scale.svelte";
  import { t } from "../lib/i18n.svelte";
  import { fadeQuick, sheetIn, sheetOut, slideIn, slideOut } from "../lib/motion";

  let {
    title,
    onclose,
    onsave,
    children,
    footer,
    media = false,
  }: {
    title: string;
    onclose: () => void;
    onsave?: () => void;
    children: Snippet;
    footer?: Snippet;
    /** Encima del lector: vidrio oscuro con los colores --media-*, en cualquier tema. */
    media?: boolean;
  } = $props();

  const phone = matchMedia("(max-width: 759px)").matches;
  const enter = (n: Element) => (phone ? sheetIn(n) : slideIn(n));
  const leave = (n: Element) => (phone ? sheetOut(n) : slideOut(n));

  function onKey(e: KeyboardEvent) {
    if (e.defaultPrevented || document.querySelector("[role=alertdialog]")) return;
    if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      onclose();
    } else if (onsave && (e.ctrlKey || e.metaKey) && (e.key === "s" || e.key === "Enter")) {
      e.preventDefault();
      (document.activeElement as HTMLElement | null)?.blur?.();
      onsave();
    }
  }

  // Arrastrar la hoja hacia abajo la cierra (celular).
  let dragY = $state(0);
  let startY = 0;
  let dragging = false;
  function down(e: PointerEvent) {
    if (!phone || e.pointerType !== "touch") return;
    dragging = true;
    startY = e.clientY;
  }
  function move(e: PointerEvent) {
    if (dragging) dragY = Math.max(0, toCss(e.clientY - startY));
  }
  function up() {
    if (!dragging) return;
    dragging = false;
    if (dragY > 90) onclose();
    dragY = 0;
  }
</script>

<svelte:window onkeydown={onKey} />

<div class="scrim" onclick={onclose} role="presentation" transition:fadeQuick|global></div>
<div
  class="drawer card"
  class:media
  class:sheet={phone}
  style={dragY ? `transform: translateY(${dragY}px); transition: none` : ""}
  in:enter|global
  out:leave|global
  role="dialog"
  aria-modal="true"
  aria-label={title}
>
  <!-- svelte-ignore a11y_no_static_element_interactions (arrastrar la hoja; cerrar tambien va con la X y Esc) -->
  <header class="titlebar" onpointerdown={down} onpointermove={move} onpointerup={up} onpointercancel={up}>
    {#if phone}<span class="grab" aria-hidden="true"></span>{/if}
    <h2>{title}</h2>
    <button class="icon-btn" onclick={onclose} aria-label={t("common.close")}><X size={16} /></button>
  </header>
  <div class="body">{@render children()}</div>
  {#if footer}
    <footer>{@render footer()}</footer>
  {/if}
</div>

<style>
  .scrim {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.35);
    z-index: 60;
  }
  .drawer {
    position: fixed;
    top: 10px;
    right: 10px;
    bottom: 10px;
    width: min(460px, calc(var(--vw) - 20px));
    z-index: 61;
    display: flex;
    flex-direction: column;
    background: var(--sidebar);
    box-shadow: var(--pop-shadow);
    transition: transform 0.25s cubic-bezier(0.2, 0.8, 0.2, 1);
  }
  .drawer.sheet {
    top: auto;
    left: 0;
    right: 0;
    bottom: 0;
    width: 100%;
    max-height: calc(var(--vh) * 0.86);
    border-radius: calc(var(--radius) + 6px) calc(var(--radius) + 6px) 0 0;
    border-bottom: 0;
    padding-bottom: env(safe-area-inset-bottom);
  }
  :global([data-theme="warp"]) .drawer,
  :global([data-theme="haze"]) .drawer {
    background: color-mix(in srgb, var(--bg) 88%, transparent);
    backdrop-filter: var(--blur);
  }
  /* En los lectores <html> va sin zoom: la hoja se agranda sola y mide la ventana con su zoom. */
  .drawer.media {
    zoom: var(--ui-zoom, 1);
    --vw: calc(100vw / var(--ui-zoom, 1));
    --vh: calc(100dvh / var(--ui-zoom, 1));
    background: rgba(18, 18, 20, 0.92);
    color: var(--on-media);
    border-color: var(--media-line);
    backdrop-filter: blur(18px);
    --text: var(--on-media);
    --text-2: var(--on-media-2);
    --text-3: color-mix(in srgb, var(--on-media) 50%, transparent);
    --panel-2: rgba(255, 255, 255, 0.08);
    --panel: rgba(255, 255, 255, 0.05);
    --line: var(--media-line);
    --line-strong: rgba(255, 255, 255, 0.32);
    --hover: rgba(255, 255, 255, 0.08);
    color-scheme: dark;
  }
  header {
    position: relative;
    display: flex;
    align-items: center;
    padding: 14px 16px;
    border-bottom: var(--border) solid var(--line);
    touch-action: none;
  }
  .grab {
    position: absolute;
    top: 6px;
    left: 50%;
    width: 38px;
    height: 4px;
    margin-left: -19px;
    border-radius: 99px;
    background: var(--line-strong);
  }
  h2 {
    flex: 1;
    margin: 0;
    font-size: 15px;
    font-weight: 650;
  }
  :global([data-theme="poolsuite"]) .drawer:not(.media) h2 {
    flex: none;
    background: var(--sidebar);
    padding: 0 10px;
    margin: 0 auto;
  }
  .body {
    flex: 1;
    overflow: auto;
    overscroll-behavior: contain;
    padding: 16px;
    display: flex;
    flex-direction: column;
    gap: 16px;
  }
  footer {
    display: flex;
    gap: 8px;
    justify-content: flex-end;
    flex-wrap: wrap;
    align-items: center;
    padding: 10px 16px 12px;
    border-top: var(--border) solid var(--line);
  }
</style>
