<script lang="ts">
  // Busqueda global (#/search?q=...&alt=...). Se escribe en la barra de arriba (SearchBar); aca
  // los resultados: primero lo propio al instante (biblioteca y libros subidos), despues una fila
  // por fuente a medida que contestan (NDJSON). Con `alt` (una sugerencia de AniList) cada fuente
  // busca tambien esos titulos; sin `alt`, el server le pregunta a AniList si lo escrito es el
  // nombre de una obra en otro idioma ("attack on titan" → "Shingeki no Kyojin").
  import { LoaderCircle, RefreshCw, Search } from "@lucide/svelte";
  import { flip } from "svelte/animate";
  import CoverCard from "../components/CoverCard.svelte";
  import Row from "../components/Row.svelte";
  import { api, errText, searchAll } from "../lib/api";
  import { t } from "../lib/i18n.svelte";
  import { flipMs, rise } from "../lib/motion";
  import { go, route } from "../lib/router.svelte";
  import { loadSources, searchSources } from "../lib/sources.svelte";
  import type { Book, LibraryEntry, SearchGroup, WorkCard } from "../lib/types";

  let q = $state("");
  let alsoAs = $state<string[]>([]);
  let groups = $state<SearchGroup[]>([]);
  let searching = $state(false);
  let error = $state("");
  let library = $state<LibraryEntry[]>([]);
  let books = $state<Book[]>([]);
  let ctrl: AbortController | null = null;

  api.library().then((l) => (library = l)).catch(() => {});
  api.books().then((b) => (books = b)).catch(() => {});

  const norm = (s: string) => s.normalize("NFD").replace(/\p{Diacritic}/gu, "").toLowerCase();
  const mine = $derived.by(() => {
    const terms = [q, ...alsoAs].map((x) => norm(x.trim())).filter((x) => x.length >= 2);
    if (!terms.length) return [];
    const hit = (s: string) => terms.some((n) => norm(s).includes(n));
    const out: WorkCard[] = [];
    const seen = new Set<string>();
    for (const e of library) if (hit(e.work.title) && !seen.has(e.work.id)) (seen.add(e.work.id), out.push(e.work));
    for (const b of books) if ((hit(b.title) || hit(b.author ?? "")) && !seen.has(b.id)) (seen.add(b.id), out.push(b));
    return out;
  });

  async function run(term: string, alts: string[]) {
    ctrl?.abort();
    q = term.trim();
    alsoAs = alts;
    groups = [];
    error = "";
    if (q.length < 2) {
      searching = false;
      return;
    }
    const c = (ctrl = new AbortController());
    searching = true;
    try {
      // Las fijadas o las de los idiomas del usuario (sin la lista de fuentes, decide el server).
      const ids = await loadSources()
        .then((l) => searchSources(l))
        .catch(() => undefined);
      if (c.signal.aborted) return;
      await searchAll(
        q,
        (g) => {
          if (c.signal.aborted) return;
          // Las que traen resultados arriba; las vacias y con error, al final.
          const score = (x: SearchGroup) => (x.items.length ? 0 : x.error ? 2 : 1);
          groups = [...groups, g].sort((a, b) => score(a) - score(b));
        },
        c.signal,
        { sources: ids?.length ? ids : undefined, alts, resolve: true, onAlts: (a) => !c.signal.aborted && (alsoAs = a) },
      );
    } catch (e) {
      if ((e as Error).name !== "AbortError") error = errText(e);
    } finally {
      if (ctrl === c) searching = false;
    }
  }

  // Cada cambio de la URL (la barra de arriba navega aca) busca de nuevo.
  let lastKey = "";
  $effect(() => {
    const term = route.query.get("q") ?? "";
    const alts = route.query.getAll("alt");
    const key = JSON.stringify([term, alts]);
    if (key === lastKey) return;
    lastKey = key;
    run(term, alts);
  });

  async function retry(g: SearchGroup) {
    const c = new AbortController();
    groups = groups.map((x) => (x.source.id === g.source.id ? { ...x, error: undefined, items: [] } : x));
    await searchAll(q, (ng) => (groups = groups.map((x) => (x.source.id === ng.source.id ? ng : x))), c.signal, { sources: [g.source.id], alts: alsoAs }).catch(() => {});
  }

  const found = $derived(groups.filter((g) => g.items.length));
  const empty = $derived(groups.filter((g) => !g.items.length));
</script>

<div class="view">
  {#if q}
    <div class="page-head">
      <h1 class="ellipsis">{t("search.resultsFor", { q })}</h1>
      {#if searching}<LoaderCircle size={18} class="spin faint" />{/if}
    </div>
    {#if alsoAs.length}
      <div class="also">
        <span class="faint">{t("search.alsoAs")}:</span>
        {#each alsoAs as a (a)}<button class="chip" onclick={() => go(`/search?q=${encodeURIComponent(a)}`)} title={t("search.onlyThis")}>{a}</button>{/each}
      </div>
    {/if}
  {/if}
  {#if !q}
    <div class="empty hint">
      <Search size={30} />
      <h3>{t("search.title")}</h3>
      <p class="muted">{t("search.hint")}</p>
    </div>
  {:else}
    {#if mine.length}
      <div class="group-host">
        <Row title={t("search.mine")} items={mine}>
          {#snippet card(w: WorkCard, i: number)}
            <CoverCard work={w} {i} sub={w.kind === "book" ? (w.format ?? "").toUpperCase() : (w.source ?? "")} />
          {/snippet}
        </Row>
      </div>
    {/if}

    {#each found as g (g.source.id)}
      <div class="group-host" in:rise animate:flip={{ duration: flipMs() }}>
        <Row title={`${g.source.name} · ${g.items.length}`} items={g.items} href={`/explore/${g.source.id}?q=${encodeURIComponent(q)}`}>
          {#snippet card(w: WorkCard, i: number)}
            <CoverCard work={w} {i} />
          {/snippet}
        </Row>
      </div>
    {/each}

    {#if searching}
      <div class="searching faint"><LoaderCircle size={15} class="spin" />{t("search.searching", { n: groups.length })}</div>
    {:else if !found.length && !mine.length && !error}
      <div class="empty"><h3>{t("search.none", { q })}</h3><p class="muted">{t("search.noneHint")}</p></div>
    {/if}
    {#if error}<div class="empty"><p class="muted">{error}</p></div>{/if}

    {#if empty.length && !searching}
      <div class="nothing">
        <div class="section-title">{t("search.noResultsIn")}</div>
        <div class="chips-scroll wrap">
          {#each empty as g (g.source.id)}
            {#if g.error}
              <button class="chip bad" title={g.error} onclick={() => retry(g)}><RefreshCw size={11} />{g.source.name}</button>
            {:else}
              <span class="chip">{g.source.name}</span>
            {/if}
          {/each}
        </div>
      </div>
    {/if}
  {/if}
</div>

<style>
  .also {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
    margin: -6px 0 18px;
    font-size: 13px;
  }
  .also .chip {
    cursor: pointer;
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
  .hint p {
    max-width: 420px;
    margin: 0;
  }
  .nothing {
    margin-top: 10px;
  }
  .wrap {
    flex-wrap: wrap;
  }
  .chip.bad {
    cursor: pointer;
  }
</style>
