<script lang="ts">
  // "Libros gratis" (pestaña de Libros): los catalogos gratis y legales (Elejandria, textos.info,
  // Internet Archive y los OPDS que agregue el admin). Con "Todos" y sin busqueda, una fila por
  // catalogo con lo popular; buscando, una fila por catalogo a medida que contestan (NDJSON). Con
  // un catalogo elegido, su grilla con scroll infinito y, si es OPDS, sus secciones (migas para
  // volver). Tocar un libro abre su detalle para agregarlo a "Mis libros".
  import { ChevronRight, Library, LoaderCircle, RefreshCw, Search, X } from "@lucide/svelte";
  import { untrack } from "svelte";
  import CatalogEntryDrawer from "./CatalogEntryDrawer.svelte";
  import CoverCard from "./CoverCard.svelte";
  import CoverGrid from "./CoverGrid.svelte";
  import Row from "./Row.svelte";
  import Switch from "./Switch.svelte";
  import { api, errText, searchCatalogs } from "../lib/api";
  import { entryWork, loadCatalogs } from "../lib/catalogs";
  import { t } from "../lib/i18n.svelte";
  import { rise } from "../lib/motion";
  import { route, setQuery } from "../lib/router.svelte";
  import { read, write } from "../lib/store";
  import type { Book, Catalog, CatalogEntry, CatalogGroup, CatalogNav } from "../lib/types";

  let { onimported }: { onimported?: (b: Book) => void } = $props();

  const SPANISH_KEY = "library.catSpanish";
  let catalogs = $state<Catalog[] | null>(null);
  let failed = $state("");
  let current = $state<string | null>(route.query.get("catalog"));
  let q = $state(route.query.get("q") ?? "");
  let spanish = $state(read(SPANISH_KEY) !== "0");
  let open = $state<{ catalog: Catalog; entry: CatalogEntry } | null>(null);

  loadCatalogs()
    .then((c) => (catalogs = c))
    .catch((e) => ((catalogs = []), (failed = errText(e))));

  const cat = $derived(catalogs?.find((c) => c.id === current) ?? null);

  // ---------- un catalogo: grilla + secciones ----------
  let entries = $state<CatalogEntry[] | null>(null);
  let nav = $state<CatalogNav[]>([]);
  let crumbs = $state<CatalogNav[]>([]);
  let next = $state<string | null>(null);
  let loading = $state(false);
  let pageError = $state("");
  let ctrl: AbortController | null = null;

  async function loadPage(c: Catalog, path: string | null, append: boolean) {
    ctrl?.abort();
    const ac = (ctrl = new AbortController());
    loading = true;
    pageError = "";
    if (!append) {
      entries = null;
      nav = [];
    }
    try {
      const p = await api.catalogBrowse(c.id, { q: q.trim() || undefined, path, all: !spanish }, ac.signal);
      if (ac.signal.aborted) return;
      entries = append ? [...(entries ?? []), ...p.entries] : p.entries;
      if (!append) nav = p.navigation;
      next = p.next;
    } catch (e) {
      if ((e as Error).name === "AbortError") return;
      pageError = errText(e);
      entries = entries ?? [];
      next = null;
    } finally {
      if (ctrl === ac) loading = false;
    }
  }

  // ---------- todos: filas ----------
  let groups = $state<CatalogGroup[]>([]);
  let searching = $state(false);

  async function searchAllCatalogs() {
    ctrl?.abort();
    const ac = (ctrl = new AbortController());
    groups = [];
    searching = true;
    try {
      await searchCatalogs(
        q.trim(),
        (g) => {
          if (ac.signal.aborted) return;
          const score = (x: CatalogGroup) => (x.entries.length ? 0 : x.error ? 2 : 1);
          groups = [...groups, g].sort((a, b) => score(a) - score(b));
        },
        ac.signal,
        { all: !spanish },
      );
    } catch (e) {
      if ((e as Error).name !== "AbortError") pageError = errText(e);
    } finally {
      if (ctrl === ac) searching = false;
    }
  }

  // La URL manda cuando cambia desde afuera (la barra de busqueda de arriba: "Buscar en los libros
  // gratis" estando ya en Libros, o atras/adelante).
  $effect(() => {
    const uq = route.query.get("q") ?? "";
    const uc = route.query.get("catalog") || null;
    if (route.query.get("tab") !== "explore") return;
    untrack(() => {
      if (uq !== q.trim()) {
        q = uq;
        typed = uq;
      }
      if (uc !== current) current = uc;
    });
  });

  // Lo que se muestra depende del catalogo, la busqueda y el filtro de idioma.
  let reloadKey = $state(0);
  $effect(() => {
    void reloadKey;
    const c = cat;
    const term = q.trim();
    void spanish;
    if (!catalogs) return;
    setQuery({ tab: "explore", catalog: current ?? "", q: term });
    crumbs = [];
    if (c) loadPage(c, null, false);
    else if (term.length >= 2) searchAllCatalogs();
    else {
      ctrl?.abort();
      groups = [];
      searching = false;
    }
  });

  // La busqueda espera a que se deje de escribir (Enter va directo).
  let typed = $state(route.query.get("q") ?? "");
  let timer = 0;
  function onType() {
    clearTimeout(timer);
    timer = window.setTimeout(() => (q = typed), 450);
  }

  function pick(id: string | null) {
    current = id;
  }

  function enter(n: CatalogNav) {
    if (!cat) return;
    crumbs = [...crumbs, n];
    loadPage(cat, n.path, false);
  }
  function crumb(i: number) {
    if (!cat) return;
    crumbs = crumbs.slice(0, i + 1);
    loadPage(cat, i < 0 ? null : crumbs[i].path, false);
  }

  function imported(e: CatalogEntry, b: Book) {
    const mark = (l: CatalogEntry[]) => l.map((x) => (x.id === e.id ? { ...x, imported: b.id } : x));
    if (entries) entries = mark(entries);
    groups = groups.map((g) => ({ ...g, entries: mark(g.entries) }));
    onimported?.(b);
  }

  function toggleSpanish(v: boolean) {
    spanish = v;
    write(SPANISH_KEY, v ? "1" : "0");
  }
</script>

<div class="cat-browser">
  <p class="legal faint"><Library size={14} />{t("cat.legal")}</p>

  {#if catalogs && !catalogs.length}
    <div class="empty">
      <Library size={30} />
      <h3>{t("cat.noCatalogs")}</h3>
      <p class="muted">{failed || t("cat.noCatalogsHint")}</p>
    </div>
  {:else}
    <div class="chips-scroll">
      {#if catalogs}
        <button class="pick" class:on={!current} onclick={() => pick(null)}>{t("cat.all")}</button>
        {#each catalogs as c (c.id)}
          <button class="pick" class:on={c.id === current} onclick={() => pick(c.id)}>
            <span>{c.name}</span>
            {#if c.spanish}<span class="lang">ES</span>{/if}
          </button>
        {/each}
      {:else}
        {#each Array(4) as _, i (i)}<span class="skel chip-skel"></span>{/each}
      {/if}
    </div>

    <div class="bar">
      <form class="inbox find" onsubmit={(e) => (e.preventDefault(), clearTimeout(timer), (q = typed))}>
        <Search size={15} />
        <input bind:value={typed} oninput={onType} placeholder={cat ? t("cat.searchIn", { name: cat.name }) : t("cat.searchAll")} enterkeyhint="search" />
        {#if typed}<button type="button" class="icon-btn" onclick={() => ((typed = ""), (q = ""))} aria-label={t("common.clear")}><X size={14} /></button>{/if}
      </form>
      <Switch checked={spanish} onchange={toggleSpanish}>{t("cat.spanishOnly")}</Switch>
    </div>

    {#if cat}
      {#if crumbs.length || nav.length}
        <div class="nav">
          {#if crumbs.length}
            <div class="crumbs">
              <button class="crumb" onclick={() => crumb(-1)}>{t("cat.home")}</button>
              {#each crumbs as c, i (c.path)}
                <ChevronRight size={13} class="faint" />
                <button class="crumb" class:last={i === crumbs.length - 1} onclick={() => crumb(i)}>{c.title}</button>
              {/each}
            </div>
          {/if}
          {#if nav.length}
            <div class="chips-scroll">
              {#each nav as n (n.path)}<button class="pick" onclick={() => enter(n)}>{n.title}<ChevronRight size={13} /></button>{/each}
            </div>
          {/if}
        </div>
      {/if}
      {#if pageError && !entries?.length}
        <div class="empty">
          <p class="muted">{pageError}</p>
          <button class="btn" onclick={() => reloadKey++}><RefreshCw size={14} />{t("common.retry")}</button>
        </div>
      {:else if entries && !entries.length && !loading && !nav.length}
        <div class="empty"><h3>{q.trim() ? t("cat.none", { q: q.trim() }) : t("cat.empty")}</h3></div>
      {:else if entries?.length || loading}
        <CoverGrid items={entries} {loading} skeletons={18} hasMore={!!next} more={() => cat && next && !loading && loadPage(cat, next, true)}>
          {#snippet card(e: CatalogEntry, i: number)}
            <CoverCard work={entryWork(e)} {i} sub={e.author ?? ""} badge={e.imported ? "✓" : ""} onopen={() => (open = { catalog: cat!, entry: e })} />
          {/snippet}
        </CoverGrid>
      {/if}
    {:else if catalogs}
      {#if q.trim().length >= 2}
        {#each groups.filter((g) => g.entries.length) as g (g.catalog.id)}
          <div class="group-host" in:rise>
            <Row title={t("search.books", { name: g.catalog.name, n: g.entries.length })} items={g.entries} href={`/books?tab=explore&catalog=${g.catalog.id}&q=${encodeURIComponent(q.trim())}`}>
              {#snippet card(e: CatalogEntry, i: number)}
                <CoverCard work={entryWork(e)} {i} sub={e.author ?? ""} badge={e.imported ? "✓" : ""} onopen={() => (open = { catalog: g.catalog, entry: e })} />
              {/snippet}
            </Row>
          </div>
        {/each}
        {#if searching}
          <div class="searching faint"><LoaderCircle size={15} class="spin" />{t("search.searching", { n: groups.length })}</div>
        {:else if !groups.some((g) => g.entries.length)}
          <div class="empty"><h3>{t("cat.none", { q: q.trim() })}</h3><p class="muted">{t("search.noneHint")}</p></div>
        {/if}
        {#if !searching && groups.some((g) => g.error)}
          <div class="chips-scroll wrap">
            {#each groups.filter((g) => g.error) as g (g.catalog.id)}
              <span class="chip bad" title={g.error}>{t("cat.failed", { name: g.catalog.name })}</span>
            {/each}
          </div>
        {/if}
      {:else}
        {#key spanish}
          {#each catalogs as c (c.id)}
            <div class="group-host">
              <Row
                title={t("cat.popular", { name: c.name })}
                load={() => api.catalogBrowse(c.id, { all: !spanish }).then((p) => p.entries)}
                href={`/books?tab=explore&catalog=${c.id}`}
              >
                {#snippet card(e: CatalogEntry, i: number)}
                  <CoverCard work={entryWork(e)} {i} sub={e.author ?? ""} badge={e.imported ? "✓" : ""} onopen={() => (open = { catalog: c, entry: e })} />
                {/snippet}
              </Row>
            </div>
          {/each}
        {/key}
      {/if}
    {/if}
  {/if}
</div>

{#if open}
  <CatalogEntryDrawer catalog={open.catalog} entry={open.entry} onclose={() => (open = null)} onimported={imported} />
{/if}

<style>
  .legal {
    display: flex;
    align-items: flex-start;
    gap: 7px;
    line-height: 1.45;
    margin: -4px 0 14px;
    font-size: 12.5px;
  }
  .legal :global(svg) {
    flex: none;
    margin-top: 2px;
  }
  .lang {
    font-size: 10.5px;
    font-weight: 700;
    opacity: 0.7;
  }
  .chip-skel {
    width: 110px;
    height: 34px;
    border-radius: 999px;
    flex: none;
  }
  .bar {
    display: flex;
    align-items: center;
    gap: 16px;
    flex-wrap: wrap;
    margin: 12px 0 18px;
  }
  .find {
    width: min(420px, 100%);
    height: 38px;
  }
  .find .icon-btn {
    width: 24px;
    height: 24px;
  }
  .nav {
    display: flex;
    flex-direction: column;
    gap: 10px;
    margin-bottom: 18px;
  }
  .crumbs {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 4px;
    font-size: 13px;
  }
  .crumb {
    color: var(--text-2);
    padding: 2px 4px;
    border-radius: var(--radius-sm);
  }
  .crumb:hover {
    color: var(--text);
    background: var(--hover);
  }
  .crumb.last {
    color: var(--text);
    font-weight: 650;
  }
  .group-host {
    margin: 0 calc(-1 * var(--gutter));
  }
  .searching {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 8px;
    padding: 18px;
  }
  .wrap {
    flex-wrap: wrap;
  }
</style>
