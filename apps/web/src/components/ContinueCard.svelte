<script lang="ts">
  // "Seguir leyendo": tarjeta apaisada con la portada desenfocada de fondo, la portada chica a la
  // izquierda, el titulo, donde quedo y la barra de progreso. Tocarla abre el lector ahi mismo;
  // tocar el titulo va a la ficha (o a los datos del libro).
  import { BookOpen } from "@lucide/svelte";
  import Img from "./Img.svelte";
  import { progressLabel } from "../lib/format";
  import { ago, t } from "../lib/i18n.svelte";
  import { press } from "../lib/press";
  import { go } from "../lib/router.svelte";
  import type { HistoryEntry } from "../lib/types";

  let { entry, i = 0, onmenu }: { entry: HistoryEntry; i?: number; onmenu?: (x: number, y: number) => void } = $props();

  const p = $derived(entry.progress);
  // Capitulo terminado con uno siguiente: se abre ese desde el principio.
  const target = $derived.by(() => {
    const w = entry.work;
    if (w.kind === "book") return `/book/${w.id}`;
    if (p.done && entry.nextChapter) return `/read/${w.id}/${entry.nextChapter}`;
    if (!p.chapter) return `/manga/${w.id}`;
    return `/read/${w.id}/${p.chapter}?page=${encodeURIComponent(p.position)}`;
  });
  const label = $derived(
    entry.work.kind === "manga" && p.done && entry.nextChapter ? t("home.nextChapter") : progressLabel(p, entry.chapterName, entry.work.kind),
  );
  const detail = $derived(entry.work.kind === "book" ? `/books?edit=${encodeURIComponent(entry.work.id)}` : `/manga/${entry.work.id}`);
  const frac = $derived(entry.work.kind === "book" ? p.fraction : p.done ? 1 : p.fraction);
</script>

<a
  class="cont"
  href={"#" + target}
  style="--i: {i}"
  onclick={(e) => {
    if (e.ctrlKey || e.metaKey) return;
    e.preventDefault();
    go(target);
  }}
  use:press={onmenu}
>
  <span class="bg" aria-hidden="true">{#if entry.work.cover}<img src={entry.work.cover} alt="" loading="lazy" />{/if}</span>
  <span class="cover"><Img src={entry.work.cover} fallback={entry.work.title} /></span>
  <span class="info">
    <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions (el teclado abre el lector con la tarjeta; la ficha tambien esta en el menu) -->
    <span
      class="title"
      title={entry.work.kind === "book" ? t("book.details") : t("reader.toDetail")}
      onclick={(e) => {
        e.preventDefault();
        e.stopPropagation();
        go(detail);
      }}>{entry.work.title}</span
    >
    <span class="where ellipsis">{label}</span>
    <span class="when">{ago(p.updatedAt)}</span>
    <span class="progress"><span class="progress-fill" style="width: {Math.round(frac * 100)}%"></span></span>
  </span>
  <span class="go" aria-hidden="true"><BookOpen size={16} /></span>
</a>

<style>
  .cont {
    position: relative;
    display: flex;
    gap: 12px;
    width: var(--card-w, 300px);
    flex: none;
    padding: 10px;
    border-radius: var(--radius);
    overflow: hidden;
    border: var(--border) solid var(--line);
    background: var(--panel);
    box-shadow: var(--card-shadow);
    isolation: isolate;
    animation: rise 0.36s cubic-bezier(0.2, 0.8, 0.2, 1) backwards;
    animation-delay: calc(min(var(--i, 0), 10) * 40ms);
    transition:
      transform 0.25s cubic-bezier(0.2, 0.8, 0.2, 1),
      border-color 0.2s,
      box-shadow 0.25s;
  }
  .bg {
    position: absolute;
    inset: -20px;
    z-index: -1;
    opacity: 0.32;
  }
  .bg img {
    width: 100%;
    height: 100%;
    object-fit: cover;
    filter: blur(22px) saturate(1.3);
  }
  .bg::after {
    content: "";
    position: absolute;
    inset: 0;
    background: linear-gradient(90deg, var(--panel) 10%, color-mix(in srgb, var(--panel) 40%, transparent));
  }
  :global([data-lite]) .bg {
    display: none;
  }
  @media (hover: hover) {
    .cont:hover {
      transform: translateY(-3px);
      border-color: color-mix(in srgb, var(--accent) 50%, var(--line));
      box-shadow: 0 14px 30px -16px var(--media-scrim);
    }
  }
  .cont:active {
    transform: scale(0.98);
  }
  .cover {
    position: relative;
    flex: none;
    width: 74px;
    aspect-ratio: 2 / 3;
    border-radius: var(--radius-sm);
    overflow: hidden;
    background: var(--panel-2);
  }
  .info {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 3px;
    padding: 2px 0;
  }
  .title {
    align-self: flex-start;
    cursor: pointer;
    text-decoration: underline;
    text-decoration-color: transparent;
    text-underline-offset: 3px;
    transition: text-decoration-color 0.15s;
    font-weight: 650;
    font-size: 14px;
    line-height: 1.25;
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }
  .title:hover {
    text-decoration-color: currentColor;
  }
  .where {
    font-size: 12.5px;
    color: var(--text-2);
  }
  .when {
    font-size: 11.5px;
    color: var(--text-3);
  }
  .progress {
    margin-top: auto;
  }
  .go {
    position: absolute;
    right: 10px;
    top: 10px;
    display: grid;
    place-items: center;
    width: 30px;
    height: 30px;
    border-radius: 50%;
    background: var(--primary-bg);
    color: var(--accent-ink);
    opacity: 0;
    transform: scale(0.8);
    transition:
      opacity 0.2s,
      transform 0.2s cubic-bezier(0.34, 1.56, 0.64, 1);
  }
  .cont:hover .go {
    opacity: 1;
    transform: none;
  }
</style>
