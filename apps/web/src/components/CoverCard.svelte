<script lang="ts">
  // Portada 2:3 con el titulo abajo. Insignias: capitulos sin leer (arriba a la derecha), formato
  // del libro (arriba a la izquierda) y una barra de progreso en el borde de abajo. Al tocarla
  // vuela al detalle (View Transitions). Mantener apretado / clic derecho: menu (`onmenu`).
  import Img from "./Img.svelte";
  import { press } from "../lib/press";
  import { go, loadView } from "../lib/router.svelte";
  import type { WorkCard } from "../lib/types";

  let {
    work,
    i = 0,
    unread = null,
    progress = null,
    sub = "",
    onmenu,
    href,
    onopen,
    badge = "",
  }: {
    work: WorkCard;
    /** Posicion en la grilla (aparicion escalonada). */
    i?: number;
    unread?: number | null;
    progress?: number | null;
    /** Segunda linea (fuente, autor). */
    sub?: string;
    onmenu?: (x: number, y: number) => void;
    /** Ruta a la que lleva (por defecto el detalle del manga o el lector del libro). */
    href?: string;
    /** En vez de navegar, esto (p. ej. abrir el detalle de un libro de un catalogo). */
    onopen?: () => void;
    /** Insignia abajo a la izquierda (p. ej. "#3" en una saga, o un tilde si ya esta). */
    badge?: string;
  } = $props();

  const target = $derived(href ?? (work.kind === "manga" ? `/manga/${work.id}` : `/book/${work.id}`));
  const vt = $derived(`cover-${work.id}`);
  // El nombre de la transicion solo en la tarjeta que se toco (dos iguales en pantalla la rompen).
  let flying = $state(false);

  function open(e: MouseEvent) {
    if (onopen) {
      e.preventDefault();
      onopen();
      return;
    }
    if (e.ctrlKey || e.metaKey || e.button === 1) return;
    e.preventDefault();
    flying = work.kind === "manga";
    go(target, { vt: flying });
  }
</script>

<a
  class="cover-card"
  href={"#" + target}
  style="--i: {i}"
  onclick={open}
  onpointerenter={() => loadView(work.kind === "manga" ? "manga" : "book")}
  use:press={onmenu}
>
  <span class="frame">
    <Img src={work.cover} alt="" fallback={work.title} fallbackSub={sub} vt={flying ? vt : ""} />
    {#if work.format}<span class="badge fmt">{work.format.toUpperCase()}</span>{/if}
    {#if badge}<span class="badge tag">{badge}</span>{/if}
    {#if unread}<span class="badge unread">{unread > 99 ? "99+" : unread}</span>{/if}
    {#if progress !== null && progress > 0}
      <span class="bar"><span style="transform: scaleX({Math.min(1, progress)})"></span></span>
    {/if}
  </span>
  <span class="meta">
    <span class="name">{work.title}</span>
    {#if sub}<span class="sub ellipsis">{sub}</span>{/if}
  </span>
</a>

<style>
  .cover-card {
    display: flex;
    flex-direction: column;
    gap: 8px;
    width: var(--card-w, 100%);
    flex: none;
    min-width: 0;
    border-radius: var(--radius);
    animation: rise 0.36s cubic-bezier(0.2, 0.8, 0.2, 1) backwards;
    animation-delay: calc(min(var(--i, 0), 14) * 30ms);
    -webkit-tap-highlight-color: transparent;
  }
  .frame {
    position: relative;
    display: block;
    aspect-ratio: 2 / 3;
    border-radius: var(--radius);
    overflow: hidden;
    background: var(--panel-2);
    border: var(--border) solid var(--line);
    box-shadow: var(--card-shadow);
    transition:
      transform 0.28s cubic-bezier(0.2, 0.8, 0.2, 1),
      box-shadow 0.28s,
      border-color 0.2s;
  }
  /* Reflejo que cruza la portada al pasar el mouse. */
  .frame::after {
    content: "";
    position: absolute;
    inset: 0;
    background: linear-gradient(115deg, transparent 35%, color-mix(in srgb, var(--on-media) 16%, transparent) 50%, transparent 65%);
    transform: translateX(-110%);
    pointer-events: none;
  }
  @media (hover: hover) {
    .cover-card:hover .frame {
      transform: translateY(-4px) scale(1.03);
      border-color: color-mix(in srgb, var(--accent) 55%, var(--line));
      box-shadow:
        0 18px 34px -16px var(--media-scrim),
        0 0 0 1px color-mix(in srgb, var(--accent) 35%, transparent),
        0 10px 40px -12px color-mix(in srgb, var(--accent) 45%, transparent);
    }
    .cover-card:hover .frame::after {
      transform: translateX(110%);
      transition: transform 0.75s ease;
    }
  }
  .cover-card:active .frame {
    transform: scale(0.98);
    transition-duration: 0.08s;
  }
  .cover-card:focus-visible {
    outline: none;
  }
  .cover-card:focus-visible .frame {
    box-shadow:
      0 0 0 2px var(--bg),
      0 0 0 4px var(--focus);
  }
  .badge {
    position: absolute;
    top: 6px;
    z-index: 1;
    padding: 2px 7px;
    border-radius: 999px;
    font-size: 10.5px;
    font-weight: 750;
    line-height: 16px;
    letter-spacing: 0.02em;
  }
  .unread {
    right: 6px;
    background: var(--primary-bg);
    color: var(--accent-ink);
    box-shadow: 0 2px 8px -2px var(--media-scrim);
  }
  .fmt {
    left: 6px;
    background: var(--media-scrim);
    color: var(--on-media);
  }
  .tag {
    top: auto;
    bottom: 10px;
    left: 6px;
    background: var(--primary-bg);
    color: var(--accent-ink);
    box-shadow: 0 2px 8px -2px var(--media-scrim);
  }
  .bar {
    position: absolute;
    left: 0;
    right: 0;
    bottom: 0;
    height: 4px;
    background: color-mix(in srgb, var(--media-bg) 55%, transparent);
  }
  .bar span {
    display: block;
    height: 100%;
    background: var(--primary-bg);
    transform-origin: left;
  }
  .meta {
    display: flex;
    flex-direction: column;
    gap: 1px;
    padding: 0 2px;
    min-width: 0;
  }
  .name {
    font-weight: 560;
    font-size: 13px;
    line-height: 1.3;
    color: var(--text);
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }
  .sub {
    font-size: 11.5px;
    color: var(--text-3);
  }
  :global([data-lite]) .frame {
    transition-property: transform, border-color;
  }
  :global([data-theme="poolsuite"]) .cover-card:hover .frame {
    box-shadow: 5px 5px 0 var(--line);
    transform: translate(-2px, -3px);
  }
  :global([data-theme="athanor"]) .cover-card:hover .frame {
    transform: translate(-4px, -4px);
    box-shadow: none;
    transition-timing-function: steps(2);
  }
</style>
