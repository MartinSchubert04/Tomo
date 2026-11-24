<script lang="ts">
  // Lista de capitulos. Puede tener 1500: solo se dibujan las filas cerca de la pantalla
  // (ventana sobre el contenedor que scrollea; las filas miden siempre ROW px). Leidos atenuados
  // con un tilde, el que se esta leyendo con un anillo de progreso. Orden ↑/↓, filtro por scan,
  // modo seleccion para marcar varios, y menu por fila (marcar leido, marcar anteriores).
  import { ArrowDownWideNarrow, ArrowUpWideNarrow, Check, CheckCheck, CircleDashed, EllipsisVertical, ListChecks, X } from "@lucide/svelte";
  import PopMenu, { type PopItem } from "./PopMenu.svelte";
  import Select from "./Select.svelte";
  import { chapterShort, chapterTitle } from "../lib/format";
  import { ago, num, t } from "../lib/i18n.svelte";
  import { press } from "../lib/press";
  import type { Chapter, Progress } from "../lib/types";

  let {
    chapters,
    progress,
    current = "",
    onopen,
    onmark,
  }: {
    /** Del mas nuevo al mas viejo (como los da la API). */
    chapters: Chapter[];
    progress: Record<string, Progress>;
    current?: string;
    onopen: (c: Chapter) => void;
    onmark: (ids: string[], done: boolean) => void;
  } = $props();

  const ROW = 56;
  const OVERSCAN = 12;

  let asc = $state(false);
  let scan = $state("");
  let selecting = $state(false);
  let selected = $state<Set<string>>(new Set());
  let menu = $state<{ x: number; y: number; c: Chapter } | null>(null);

  const scans = $derived([...new Set(chapters.map((c) => c.scanlator).filter((s): s is string => !!s))].sort());
  const list = $derived.by(() => {
    const l = scan ? chapters.filter((c) => c.scanlator === scan) : chapters;
    return asc ? [...l].reverse() : l;
  });
  const readCount = $derived(chapters.reduce((n, c) => n + (progress[c.id]?.done ? 1 : 0), 0));

  // Ventana: que filas se ven segun el scroll del contenedor.
  let root = $state<HTMLElement>();
  let start = $state(0);
  let end = $state(40);
  $effect(() => {
    if (!root) return;
    const el = root;
    let scroller: HTMLElement | Window = window;
    for (let p = el.parentElement; p; p = p.parentElement) {
      const o = getComputedStyle(p).overflowY;
      if (o === "auto" || o === "scroll") {
        scroller = p;
        break;
      }
    }
    let raf = 0;
    const measure = () => {
      raf = 0;
      const top = el.getBoundingClientRect().top;
      const viewH = scroller instanceof Window ? innerHeight : scroller.getBoundingClientRect().bottom;
      const first = Math.floor(-top / ROW);
      const visible = Math.ceil(viewH / ROW);
      start = Math.max(0, first - OVERSCAN);
      end = Math.max(start, first + visible + OVERSCAN);
    };
    const on = () => {
      if (!raf) raf = requestAnimationFrame(measure);
    };
    measure();
    scroller.addEventListener("scroll", on, { passive: true });
    addEventListener("resize", on);
    return () => {
      scroller.removeEventListener("scroll", on);
      removeEventListener("resize", on);
      cancelAnimationFrame(raf);
    };
  });
  const slice = $derived(list.slice(start, Math.min(end, list.length)));

  function toggle(id: string) {
    const s = new Set(selected);
    if (s.has(id)) s.delete(id);
    else s.add(id);
    selected = s;
  }

  function finishSelect(done: boolean) {
    onmark([...selected], done);
    selected = new Set();
    selecting = false;
  }

  function menuItems(c: Chapter): PopItem[] {
    const done = !!progress[c.id]?.done;
    // "Anteriores": los de numero menor (los que estan despues en la lista original).
    const i = chapters.findIndex((x) => x.id === c.id);
    const older = chapters.slice(i + 1).map((x) => x.id);
    const items: PopItem[] = [
      { label: done ? t("manga.markUnread") : t("manga.markRead"), icon: done ? CircleDashed : Check, run: () => onmark([c.id], !done) },
    ];
    if (older.length) items.push({ label: t("manga.markPrevious"), icon: CheckCheck, run: () => onmark(older, true) });
    items.push({
      label: t("manga.select"),
      icon: ListChecks,
      run: () => {
        selecting = true;
        selected = new Set([c.id]);
      },
    });
    return items;
  }
</script>

<div class="ch-head">
  <h2>{t("manga.chapters", { n: chapters.length })}</h2>
  {#if chapters.length}<span class="faint count">{t("manga.readOf", { read: num(readCount), n: num(chapters.length) })}</span>{/if}
  <span class="grow"></span>
  {#if scans.length > 1}
    <div class="scan">
      <Select
        bind:value={scan}
        options={[{ value: "", label: t("manga.allScans") }, ...scans.map((s) => ({ value: s, label: s }))]}
      />
    </div>
  {/if}
  <button class="icon-btn" title={asc ? t("manga.sortDesc") : t("manga.sortAsc")} onclick={() => (asc = !asc)}>
    {#if asc}<ArrowUpWideNarrow size={17} />{:else}<ArrowDownWideNarrow size={17} />{/if}
  </button>
  <button class="icon-btn" class:on={selecting} title={t("manga.select")} onclick={() => ((selecting = !selecting), (selected = new Set()))}>
    <ListChecks size={17} />
  </button>
</div>

{#if selecting}
  <div class="selbar card">
    <button class="icon-btn" onclick={() => ((selecting = false), (selected = new Set()))} aria-label={t("common.cancel")}><X size={16} /></button>
    <span class="grow">{t("manga.selected", { n: selected.size })}</span>
    <button class="btn sm" onclick={() => (selected = new Set(list.map((c) => c.id)))}>{t("manga.selectAll")}</button>
    <button class="btn sm" disabled={!selected.size} onclick={() => finishSelect(false)}>{t("manga.markUnread")}</button>
    <button class="btn sm primary" disabled={!selected.size} onclick={() => finishSelect(true)}>{t("manga.markRead")}</button>
  </div>
{/if}

{#if !list.length}
  <div class="empty-ch faint">{t("manga.noChapters")}</div>
{/if}
<div class="list" bind:this={root} style="height: {list.length * ROW}px">
  <div class="win" style="transform: translateY({start * ROW}px)">
    {#each slice as c (c.id)}
      {@const p = progress[c.id]}
      {@const reading = !!p && !p.done && Number(p.position) > 0}
      <div
        class="ch"
        class:read={p?.done}
        class:current={c.id === current}
        class:sel={selected.has(c.id)}
        role="button"
        tabindex="0"
        onclick={() => (selecting ? toggle(c.id) : onopen(c))}
        onkeydown={(e) => e.key === "Enter" && (selecting ? toggle(c.id) : onopen(c))}
        use:press={selecting ? undefined : (x, y) => (menu = { x, y, c })}
      >
        {#if selecting}
          <input type="checkbox" checked={selected.has(c.id)} tabindex="-1" onclick={(e) => e.stopPropagation()} onchange={() => toggle(c.id)} />
        {/if}
        <span class="txt">
          <span class="name ellipsis">
            {chapterShort(c)}{#if chapterTitle(c)}<span class="title">{" · " + chapterTitle(c)}</span>{/if}
          </span>
          <span class="meta ellipsis">
            {#if c.scanlator}{c.scanlator}{/if}{#if c.scanlator && c.date}{" · "}{/if}{#if c.date}{ago(c.date)}{/if}
          </span>
        </span>
        {#if reading}
          <span class="ring" style="--p: {Math.round((p?.fraction ?? 0) * 100)}" title={t("manga.inProgress")}></span>
        {:else if p?.done}
          <span class="tick"><Check size={15} /></span>
        {/if}
        {#if !selecting}
          <button
            class="icon-btn more"
            aria-label={t("common.more")}
            onclick={(e) => {
              e.stopPropagation();
              const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
              menu = { x: r.left, y: r.bottom + 4, c };
            }}><EllipsisVertical size={16} /></button
          >
        {/if}
      </div>
    {/each}
  </div>
</div>

{#if menu}
  <PopMenu x={menu.x} y={menu.y} items={menuItems(menu.c)} onclose={() => (menu = null)} />
{/if}

<style>
  .ch-head {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 0 0 8px;
    flex-wrap: wrap;
  }
  h2 {
    font-family: var(--font);
    font-size: 16px;
    font-weight: 650;
    margin: 0;
  }
  .count {
    font-size: 12px;
  }
  .scan {
    width: min(220px, 50vw);
  }
  .icon-btn {
    width: 34px;
    height: 34px;
  }
  .icon-btn.on {
    color: var(--accent);
    background: var(--hover);
  }
  .selbar {
    position: sticky;
    top: 8px;
    z-index: 5;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 10px;
    margin-bottom: 8px;
    background: var(--float, var(--sidebar));
    box-shadow: var(--pop-shadow);
    flex-wrap: wrap;
  }
  .list {
    position: relative;
    contain: strict;
    content-visibility: visible;
    width: 100%;
  }
  .win {
    position: absolute;
    left: 0;
    right: 0;
    top: 0;
    will-change: transform;
  }
  .ch {
    display: flex;
    align-items: center;
    gap: 12px;
    height: 56px;
    padding: 0 6px 0 12px;
    border-radius: var(--radius-sm);
    cursor: pointer;
    transition: background 0.12s;
    -webkit-tap-highlight-color: transparent;
  }
  .ch:hover {
    background: var(--hover);
  }
  .ch:active {
    background: color-mix(in srgb, var(--accent) 10%, transparent);
  }
  .ch.current {
    background: color-mix(in srgb, var(--accent) 9%, transparent);
    box-shadow: inset 3px 0 0 var(--accent);
  }
  .ch.sel {
    background: color-mix(in srgb, var(--accent) 14%, transparent);
  }
  .txt {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .name {
    font-weight: 580;
    font-size: 13.5px;
  }
  .title {
    font-weight: 450;
    color: var(--text-2);
  }
  .meta {
    font-size: 11.5px;
    color: var(--text-3);
  }
  .read .name,
  .read .title {
    color: var(--text-3);
  }
  .tick {
    color: var(--ok);
    display: grid;
  }
  .ring {
    width: 18px;
    height: 18px;
    flex: none;
    border-radius: 50%;
    background: conic-gradient(var(--accent) calc(var(--p) * 1%), color-mix(in srgb, var(--text) 15%, transparent) 0);
    mask: radial-gradient(circle, transparent 5px, #000 5.5px);
  }
  .more {
    opacity: 0.6;
  }
  .ch:hover .more {
    opacity: 1;
  }
  .empty-ch {
    padding: 30px 0;
    text-align: center;
  }
</style>
