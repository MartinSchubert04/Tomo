<script lang="ts">
  // Lista de capitulos, una fila por numero (lib/chapters.ts): si un numero viene en varios
  // idiomas o de varios scans, la fila abre la version por defecto y la pastilla de la izquierda
  // ("LAT", "EN" y cuantas versiones hay) deja elegir otra. Puede tener 1500 filas: solo se
  // dibujan las cercanas a la pantalla (ventana sobre el contenedor que scrollea; las filas miden
  // siempre ROW px). Leidos atenuados con un tilde (cualquier version leida cuenta), el que se
  // esta leyendo con un anillo de progreso. Orden, "Preferir scan", modo seleccion para marcar
  // varios y menu por fila (marcar leido, marcar anteriores).
  import { ArrowDownWideNarrow, ArrowUpWideNarrow, Check, CheckCheck, CircleDashed, EllipsisVertical, ListChecks, X } from "@lucide/svelte";
  import PopMenu, { type PopItem } from "./PopMenu.svelte";
  import Select from "./Select.svelte";
  import { groupChapters, groupDone, groupProgress, langBadge, pickVariant, preferredScan, setPreferredScan, type ChapterGroup } from "../lib/chapters";
  import { chapterShort, chapterTitle } from "../lib/format";
  import { ago, num, t } from "../lib/i18n.svelte";
  import { press } from "../lib/press";
  import { zf } from "../lib/scale.svelte";
  import type { Chapter, Progress } from "../lib/types";

  let {
    work,
    chapters,
    progress,
    current = "",
    onopen,
    onmark,
  }: {
    /** El manga (para recordar el scan preferido). */
    work: string;
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
  // svelte-ignore state_referenced_locally
  let scan = $state(preferredScan(work) ?? "");
  let selecting = $state(false);
  let selected = $state<Set<string>>(new Set());
  let menu = $state<{ x: number; y: number; items: PopItem[] } | null>(null);

  const groups = $derived(groupChapters(chapters));
  const scans = $derived([...new Set(chapters.map((c) => c.scanlator).filter((s): s is string => !!s))].sort());
  const list = $derived(asc ? [...groups].reverse() : groups);
  const multi = $derived(groups.some((g) => g.variants.length > 1 || !!g.variants[0].lang));
  const readCount = $derived(groups.reduce((n, g) => n + (groupDone(g, progress) ? 1 : 0), 0));
  const pick = (g: ChapterGroup) => pickVariant(g, { preferScan: scan || null });

  function setScan(v: string) {
    scan = v;
    setPreferredScan(work, v || null);
  }

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
      // Rects en px de la ventana; las filas miden ROW px CSS, que con zoom ocupan ROW * zoom.
      const row = ROW * zf();
      const top = el.getBoundingClientRect().top;
      const viewH = scroller instanceof Window ? innerHeight : scroller.getBoundingClientRect().bottom;
      const first = Math.floor(-top / row);
      const visible = Math.ceil(viewH / row);
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

  const ids = (g: ChapterGroup) => g.variants.map((v) => v.id);

  function toggle(g: ChapterGroup) {
    const s = new Set(selected);
    if (s.has(g.key)) s.delete(g.key);
    else s.add(g.key);
    selected = s;
  }

  function finishSelect(done: boolean) {
    onmark(groups.filter((g) => selected.has(g.key)).flatMap(ids), done);
    selected = new Set();
    selecting = false;
  }

  function rowMenu(g: ChapterGroup, x: number, y: number) {
    const done = groupDone(g, progress);
    // "Anteriores": los de numero menor (los que estan despues en la lista original).
    const i = groups.indexOf(g);
    const older = groups.slice(i + 1).flatMap(ids);
    const items: PopItem[] = [
      { label: done ? t("manga.markUnread") : t("manga.markRead"), icon: done ? CircleDashed : Check, run: () => onmark(ids(g), !done) },
    ];
    if (older.length) items.push({ label: t("manga.markPrevious"), icon: CheckCheck, run: () => onmark(older, true) });
    items.push({
      label: t("manga.select"),
      icon: ListChecks,
      run: () => {
        selecting = true;
        selected = new Set([g.key]);
      },
    });
    menu = { x, y, items };
  }

  /** El menu de versiones: idioma, scan y fecha de cada una. */
  function variantMenu(g: ChapterGroup, x: number, y: number) {
    const def = pick(g);
    menu = {
      x,
      y,
      items: g.variants.map((v) => ({
        label: [langBadge(v.lang) || t("manga.sourceLang"), v.scanlator ?? t("manga.noScan"), v.date ? ago(v.date) : ""].filter(Boolean).join(" · "),
        icon: v.id === def.id ? Check : progress[v.id]?.done ? CheckCheck : undefined,
        title: v.name,
        run: () => onopen(v),
      })),
    };
  }

  function at(e: MouseEvent): [number, number] {
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    return [r.left, r.bottom + 4];
  }
</script>

<div class="ch-head">
  <h2>{t("manga.chapters", { n: groups.length })}</h2>
  {#if groups.length}<span class="faint count">{t("manga.readOf", { read: num(readCount), n: num(groups.length) })}</span>{/if}
  <span class="grow"></span>
  {#if scans.length > 1}
    <div class="scan">
      <Select
        value={scan}
        onchange={setScan}
        options={[{ value: "", label: t("manga.anyScan") }, ...scans.map((s) => ({ value: s, label: t("manga.preferScan", { scan: s }) }))]}
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
    <button class="btn sm" onclick={() => (selected = new Set(list.map((g) => g.key)))}>{t("manga.selectAll")}</button>
    <button class="btn sm" disabled={!selected.size} onclick={() => finishSelect(false)}>{t("manga.markUnread")}</button>
    <button class="btn sm primary" disabled={!selected.size} onclick={() => finishSelect(true)}>{t("manga.markRead")}</button>
  </div>
{/if}

{#if !list.length}
  <div class="empty-ch faint">{t("manga.noChapters")}</div>
{/if}
<div class="list" bind:this={root} style="height: {list.length * ROW}px">
  <div class="win" style="transform: translateY({start * ROW}px)">
    {#each slice as g (g.key)}
      {@const c = pick(g)}
      {@const p = groupProgress(g, progress)}
      {@const done = groupDone(g, progress)}
      {@const reading = !!p && !p.done && Number(p.position) > 0}
      <div
        class="ch"
        class:read={done}
        class:current={g.variants.some((v) => v.id === current)}
        class:sel={selected.has(g.key)}
        role="button"
        tabindex="0"
        title={c.name}
        onclick={() => (selecting ? toggle(g) : onopen(c))}
        onkeydown={(e) => e.key === "Enter" && (selecting ? toggle(g) : onopen(c))}
        use:press={selecting ? undefined : (x, y) => rowMenu(g, x, y)}
      >
        {#if selecting}
          <input type="checkbox" checked={selected.has(g.key)} tabindex="-1" onclick={(e) => e.stopPropagation()} onchange={() => toggle(g)} />
        {:else if multi}
          {#if g.variants.length > 1}
            <button
              class="lang"
              title={t("manga.versions", { n: g.variants.length })}
              aria-label={t("manga.versions", { n: g.variants.length })}
              onclick={(e) => {
                e.stopPropagation();
                variantMenu(g, ...at(e));
              }}>{langBadge(c.lang) || "·"}<span class="dot">{g.variants.length}</span></button
            >
          {:else}
            <span class="lang still">{langBadge(c.lang)}</span>
          {/if}
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
        {:else if done}
          <span class="tick" title={t("manga.read")}><Check size={15} /></span>
        {/if}
        {#if !selecting}
          <button
            class="icon-btn more"
            aria-label={t("common.more")}
            title={t("common.more")}
            onclick={(e) => {
              e.stopPropagation();
              rowMenu(g, ...at(e));
            }}><EllipsisVertical size={16} /></button
          >
        {/if}
      </div>
    {/each}
  </div>
</div>

{#if menu}
  <PopMenu x={menu.x} y={menu.y} items={menu.items} onclose={() => (menu = null)} />
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
    width: min(220px, calc(var(--vw) * 0.5));
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
  /* La pastilla del idioma (y cuantas versiones hay de ese numero). */
  .lang {
    position: relative;
    flex: none;
    min-width: 38px;
    height: 26px;
    padding: 0 6px;
    display: inline-grid;
    place-items: center;
    border-radius: 7px;
    font-family: var(--mono);
    font-size: 10.5px;
    font-weight: 700;
    letter-spacing: 0.03em;
    color: var(--text-2);
    background: var(--panel-2);
    border: var(--border) solid var(--line);
    transition:
      border-color 0.15s,
      color 0.15s,
      transform 0.15s;
  }
  button.lang:hover {
    color: var(--text);
    border-color: var(--accent);
    transform: translateY(-1px);
  }
  .lang.still {
    background: none;
    border-color: transparent;
    color: var(--text-3);
  }
  .dot {
    position: absolute;
    top: -6px;
    right: -6px;
    min-width: 15px;
    height: 15px;
    padding: 0 3px;
    border-radius: 999px;
    font-size: 9px;
    line-height: 15px;
    text-align: center;
    color: var(--accent-ink);
    background: var(--accent);
  }
  .read .lang {
    opacity: 0.6;
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
