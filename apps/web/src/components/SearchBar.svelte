<script lang="ts" module>
  /** Para "/" y Ctrl K (App.svelte): enfoca la barra de arriba, o abre la de pantalla completa. */
  export const searchFocus = $state({ n: 0 });
  export function focusSearch() {
    searchFocus.n++;
  }
</script>

<script lang="ts">
  // La busqueda, protagonista de la app: centrada arriba, se ensancha al pasar el mouse y al
  // escribir abre un panel de sugerencias en vivo (como una barra de comandos):
  //  - primera fila: "Buscar «...» en todas las fuentes" (Enter);
  //  - lo que ya existe aca (biblioteca, historial, libros) y titulos de AniList en otros idiomas
  //    (buscar "attack on titan" sugiere "Shingeki no Kyojin"). Elegir un titulo busca en las
  //    fuentes con todos sus nombres (/search?q=&alt=).
  // ↑/↓ para moverse, Enter para elegir, Esc para cerrar. Pedidos con 200 ms de espera y
  // cancelables; resultados cacheados por texto mientras la pagina esta abierta. En el celular es
  // la misma barra a pantalla completa (`full`).
  import { ArrowRight, BookOpen, CornerDownLeft, LoaderCircle, Search, Sparkles, X } from "@lucide/svelte";
  import { api } from "../lib/api";
  import { t } from "../lib/i18n.svelte";
  import { fadeQuick, pop } from "../lib/motion";
  import { go, route } from "../lib/router.svelte";
  import type { Suggestion } from "../lib/types";

  let { full = false, onclose, autofocus = false }: { full?: boolean; onclose?: () => void; autofocus?: boolean } = $props();

  // svelte-ignore state_referenced_locally
  let typed = $state(route.view === "search" ? (route.query.get("q") ?? "") : "");
  let open = $state(false);
  let active = $state(0);
  let items = $state<Suggestion[]>([]);
  let loading = $state(false);
  let input = $state<HTMLInputElement>();
  let root = $state<HTMLElement>();

  const cache = new Map<string, Suggestion[]>();
  let timer = 0;
  let ctrl: AbortController | null = null;

  // Al volver a la busqueda (atras), la barra muestra lo buscado; afuera, vacia.
  $effect(() => {
    const q = route.view === "search" ? (route.query.get("q") ?? "") : "";
    if (document.activeElement !== input) typed = q;
  });

  // "/" y Ctrl K desde cualquier lado.
  let seen = searchFocus.n;
  $effect(() => {
    if (searchFocus.n !== seen) {
      seen = searchFocus.n;
      input?.focus();
      input?.select();
    }
  });
  $effect(() => {
    if (autofocus) input?.focus();
  });

  const q = $derived(typed.trim());

  function fetchSoon() {
    clearTimeout(timer);
    ctrl?.abort();
    active = 0;
    if (q.length < 2) {
      items = [];
      loading = false;
      return;
    }
    const hit = cache.get(q.toLowerCase());
    if (hit) {
      items = hit;
      loading = false;
      return;
    }
    loading = true;
    const term = q;
    timer = window.setTimeout(async () => {
      const c = (ctrl = new AbortController());
      try {
        const r = await api.suggest(term, c.signal);
        cache.set(term.toLowerCase(), r);
        if (!c.signal.aborted && term === q) items = r;
      } catch {
        if (!c.signal.aborted) items = [];
      } finally {
        if (ctrl === c) loading = false;
      }
    }, 200);
  }

  function oninput() {
    open = true;
    fetchSoon();
  }

  /** Las filas del panel: 0 = "Buscar en todas las fuentes", despues las sugerencias. */
  const rows = $derived(q ? items.length + 1 : 0);

  function searchAll() {
    if (!q) return;
    close();
    go(`/search?q=${encodeURIComponent(q)}`, { replace: route.view === "search" });
  }

  function choose(s: Suggestion) {
    close();
    if (s.kind === "work" && s.workId) {
      go(s.workId.startsWith("b") ? `/book/${s.workId}` : `/manga/${s.workId}`);
      return;
    }
    // Un titulo de AniList: se busca con todos sus nombres.
    const p = new URLSearchParams({ q: s.title });
    for (const a of s.altTitles.slice(0, 4)) p.append("alt", a);
    typed = s.title;
    go(`/search?${p.toString()}`, { replace: route.view === "search" });
  }

  function pick(i: number) {
    if (i === 0) searchAll();
    else if (items[i - 1]) choose(items[i - 1]);
  }

  function close() {
    open = false;
    input?.blur();
    onclose?.();
  }

  function onkey(e: KeyboardEvent) {
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      if (!rows) return;
      e.preventDefault();
      open = true;
      active = (active + (e.key === "ArrowDown" ? 1 : -1) + rows) % rows;
      root?.querySelector<HTMLElement>(`[data-i="${active}"]`)?.scrollIntoView({ block: "nearest" });
    } else if (e.key === "Enter") {
      e.preventDefault();
      pick(open ? active : 0);
    } else if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      if (open && q && !full) open = false;
      else close();
    }
  }

  function outside(e: PointerEvent) {
    if (!full && open && root && !root.contains(e.target as Node)) open = false;
  }

  /** Parte el texto en [antes, coincidencia, despues] (sin importar mayusculas ni tildes). */
  function parts(text: string): [string, string, string] {
    const fold = (s: string) => s.normalize("NFD").replace(/\p{Diacritic}/gu, "").toLowerCase();
    // Se compara caracter a caracter sobre el texto sin tildes; si el largo cambia (letras
    // compuestas raras), sin resaltar.
    const a = fold(text);
    const b = fold(q);
    const i = b ? a.indexOf(b) : -1;
    if (i < 0 || a.length !== text.length) return [text, "", ""];
    return [text.slice(0, i), text.slice(i, i + q.length), text.slice(i + q.length)];
  }

  const formatLabel = (s: Suggestion) => (s.format ? t(`suggest.format.${s.format}` as Parameters<typeof t>[0]) : "");
</script>

<svelte:window onpointerdown={outside} />

<div class="sb" class:full class:open={open && !!q} bind:this={root}>
  <div class="sbf" class:focus={open}>
    <Search size={17} />
    <input
      bind:this={input}
      bind:value={typed}
      {oninput}
      onfocus={() => {
        open = true;
        if (q && !items.length) fetchSoon();
      }}
      onkeydown={onkey}
      placeholder={t("search.placeholder")}
      enterkeyhint="search"
      autocomplete="off"
      spellcheck="false"
      role="combobox"
      aria-expanded={open && !!q}
      aria-controls="sb-panel"
      aria-label={t("nav.search")}
    />
    {#if loading}<LoaderCircle size={15} class="spin faint" />{/if}
    {#if typed}
      <button
        class="icon-btn clear"
        aria-label={t("common.clear")}
        title={t("common.clear")}
        onclick={() => {
          typed = "";
          items = [];
          input?.focus();
        }}><X size={15} /></button
      >
    {:else if !full}
      <kbd title={t("search.shortcut")}>/</kbd>
    {/if}
    {#if full}
      <button class="btn sm ghost" onclick={close}>{t("common.cancel")}</button>
    {/if}
  </div>

  {#if open && q}
    <div class="panel" id="sb-panel" role="listbox" in:pop={{ start: 0.97 }} out:fadeQuick>
      <button class="opt all" class:on={active === 0} data-i="0" role="option" aria-selected={active === 0} onpointerenter={() => (active = 0)} onclick={() => pick(0)}>
        <span class="ico"><Search size={16} /></span>
        <span class="txt">{t("suggest.searchAll", { q })}</span>
        <span class="hint"><CornerDownLeft size={13} /></span>
      </button>
      {#if loading && !items.length}
        {#each Array(3) as _, i (i)}
          <div class="opt skel-row" aria-hidden="true">
            <span class="skel thumb"></span>
            <span class="lines"><span class="skel line" style="width: {55 - i * 9}%"></span><span class="skel line sm" style="width: {35 + i * 6}%"></span></span>
          </div>
        {/each}
      {/if}
      {#each items as s, i (s.kind + (s.workId ?? s.title))}
        {@const [a, m, b] = parts(s.title)}
        <button class="opt" class:on={active === i + 1} data-i={i + 1} role="option" aria-selected={active === i + 1} onpointerenter={() => (active = i + 1)} onclick={() => choose(s)}>
          <span class="thumb">
            {#if s.cover}<img src={s.cover} alt="" loading="lazy" />{:else}<BookOpen size={16} />{/if}
          </span>
          <span class="lines">
            <span class="title ellipsis">{a}<mark>{m}</mark>{b}</span>
            <span class="meta ellipsis">
              {#if s.kind === "title"}<Sparkles size={11} />{/if}
              {[s.kind === "work" ? s.source : formatLabel(s), s.year, s.altTitles.length ? t("suggest.aka", { titles: s.altTitles.slice(0, 2).join(" · ") }) : ""]
                .filter(Boolean)
                .join(" · ")}
            </span>
          </span>
          <span class="hint go"><ArrowRight size={14} /></span>
        </button>
      {/each}
      {#if !loading && !items.length && q.length >= 2}
        <div class="none faint">{t("suggest.none")}</div>
      {/if}
    </div>
  {/if}
</div>

<style>
  .sb {
    position: relative;
    width: min(100%, 560px);
    transition: width 0.26s cubic-bezier(0.2, 0.8, 0.2, 1);
  }
  /* Se ensancha al pasar el mouse o al escribir (solo un elemento cambia de ancho). */
  .sb:not(.full):hover,
  .sb:not(.full):focus-within {
    width: min(100%, 680px);
  }
  .sbf {
    display: flex;
    align-items: center;
    gap: 10px;
    height: 42px;
    padding: 0 8px 0 15px;
    border-radius: 999px;
    color: var(--text-3);
    background: color-mix(in srgb, var(--panel-2) 85%, transparent);
    border: var(--border) solid var(--line);
    box-shadow: 0 1px 0 color-mix(in srgb, var(--text) 4%, transparent) inset;
    transition:
      border-color 0.2s,
      box-shadow 0.2s,
      background 0.2s;
  }
  .sbf:hover {
    border-color: var(--line-strong);
  }
  .sbf.focus,
  .sbf:focus-within {
    color: var(--text-2);
    border-color: color-mix(in srgb, var(--accent) 65%, var(--line));
    background: var(--panel-2);
    box-shadow:
      0 0 0 4px color-mix(in srgb, var(--accent) 14%, transparent),
      0 12px 32px -18px var(--media-scrim);
  }
  .sbf input,
  .sbf input:focus,
  .sbf input:focus-visible {
    flex: 1;
    min-width: 0;
    height: 40px;
    padding: 0;
    border: 0;
    background: none;
    box-shadow: none;
    outline: none;
    font-size: 14.5px;
  }
  .clear {
    width: 28px;
    height: 28px;
  }
  kbd {
    margin-right: 6px;
  }
  .open .sbf {
    border-radius: 21px 21px 21px 21px;
  }
  /* El panel: vidrio sobrio (sin blur en equipos flojos), crece desde la barra. */
  .panel {
    position: absolute;
    left: 0;
    right: 0;
    top: calc(100% + 8px);
    z-index: 70;
    display: flex;
    flex-direction: column;
    padding: 6px;
    max-height: min(560px, calc(var(--vh) - 120px));
    overflow-y: auto;
    border-radius: calc(var(--radius) + 6px);
    background: color-mix(in srgb, var(--float, var(--sidebar)) 86%, transparent);
    backdrop-filter: blur(22px) saturate(150%);
    border: var(--border) solid var(--line);
    box-shadow: var(--pop-shadow);
    transform-origin: top center;
  }
  :global([data-lite]) .panel {
    backdrop-filter: none;
    background: var(--float, var(--sidebar));
  }
  .opt {
    display: flex;
    align-items: center;
    gap: 12px;
    min-height: 52px;
    padding: 6px 10px;
    border-radius: var(--radius-sm);
    text-align: left;
    color: var(--text);
    transition: background 0.12s;
  }
  .opt.on {
    background: color-mix(in srgb, var(--accent) 12%, transparent);
  }
  .opt.all {
    min-height: 44px;
    font-weight: 600;
  }
  .ico {
    width: 36px;
    display: grid;
    place-items: center;
    color: var(--accent);
  }
  .txt {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .thumb {
    flex: none;
    width: 36px;
    height: 52px;
    border-radius: 5px;
    overflow: hidden;
    display: grid;
    place-items: center;
    background: var(--panel-2);
    color: var(--text-3);
  }
  .thumb img {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }
  .lines {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 3px;
  }
  .title {
    font-weight: 600;
  }
  mark {
    background: none;
    color: var(--accent);
  }
  .meta {
    display: flex;
    align-items: center;
    gap: 5px;
    font-size: 12px;
    color: var(--text-3);
  }
  .meta :global(svg) {
    flex: none;
    color: var(--accent);
  }
  .hint {
    color: var(--text-3);
    opacity: 0;
    transform: translateX(-4px);
    transition:
      opacity 0.15s,
      transform 0.15s;
  }
  .opt.on .hint {
    opacity: 1;
    transform: none;
  }
  .skel-row .thumb {
    width: 36px;
    height: 52px;
  }
  .skel.line {
    height: 11px;
  }
  .skel.line.sm {
    height: 9px;
  }
  .none {
    padding: 12px 14px;
    font-size: 13px;
  }

  /* Celular: pantalla completa, la lista ocupa todo debajo de la barra. */
  .sb.full {
    width: 100%;
  }
  .full .panel {
    position: static;
    margin-top: 10px;
    max-height: none;
    border: 0;
    box-shadow: none;
    background: none;
    backdrop-filter: none;
    padding: 0;
  }
  .full .opt {
    min-height: 58px;
  }
</style>
