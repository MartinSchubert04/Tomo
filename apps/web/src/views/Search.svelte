<script lang="ts">
  // Busqueda global. Primero lo propio al instante (biblioteca y libros subidos), despues una fila
  // por fuente a medida que contestan (el server las manda por NDJSON). Se busca al apretar Enter
  // o 700 ms despues de dejar de escribir.
  import { LoaderCircle, RefreshCw, Search, X } from "@lucide/svelte";
  import { flip } from "svelte/animate";
  import CoverCard from "../components/CoverCard.svelte";
  import Row from "../components/Row.svelte";
  import { api, errText, searchAll } from "../lib/api";
  import { t } from "../lib/i18n.svelte";
  import { flipMs, rise } from "../lib/motion";
  import { route, setQuery } from "../lib/router.svelte";
  import type { Book, LibraryEntry, SearchGroup, WorkCard } from "../lib/types";

  let typed = $state(route.query.get("q") ?? "");
  let q = $state(route.query.get("q") ?? "");
  let groups = $state<SearchGroup[]>([]);
  let searching = $state(false);
  let error = $state("");
  let input = $state<HTMLInputElement>();
  let library = $state<LibraryEntry[]>([]);
  let books = $state<Book[]>([]);
  let ctrl: AbortController | null = null;
  let timer = 0;

  api.library().then((l) => (library = l)).catch(() => {});
  api.books().then((b) => (books = b)).catch(() => {});

  $effect(() => {
    if (!matchMedia("(pointer: coarse)").matches || !q) input?.focus();
  });

  const norm = (s: string) => s.normalize("NFD").replace(/\p{Diacritic}/gu, "").toLowerCase();
  const mine = $derived.by(() => {
    const n = norm(q.trim());
    if (n.length < 2) return [];
    const out: WorkCard[] = [];
    const seen = new Set<string>();
    for (const e of library) if (norm(e.work.title).includes(n) && !seen.has(e.work.id)) (seen.add(e.work.id), out.push(e.work));
    for (const b of books)
      if ((norm(b.title).includes(n) || norm(b.author ?? "").includes(n)) && !seen.has(b.id)) (seen.add(b.id), out.push(b));
    return out;
  });

  async function run(term: string) {
    ctrl?.abort();
    clearTimeout(timer);
    q = term.trim();
    setQuery({ q });
    groups = [];
    error = "";
    if (q.length < 2) {
      searching = false;
      return;
    }
    const c = (ctrl = new AbortController());
    searching = true;
    try {
      await searchAll(
        q,
        (g) => {
          if (c.signal.aborted) return;
          // Las que traen resultados arriba; las vacias y con error, al final.
          const score = (x: SearchGroup) => (x.items.length ? 0 : x.error ? 2 : 1);
          groups = [...groups, g].sort((a, b) => score(a) - score(b));
        },
        c.signal,
      );
    } catch (e) {
      if ((e as Error).name !== "AbortError") error = errText(e);
    } finally {
      if (ctrl === c) searching = false;
    }
  }

  // Al abrir con ?q= (atras desde un manga) se busca de nuevo.
  const initial = route.query.get("q") ?? "";
  if (initial) run(initial);

  function onInput() {
    clearTimeout(timer);
    timer = window.setTimeout(() => run(typed), 700);
  }

  async function retry(g: SearchGroup) {
    const c = new AbortController();
    groups = groups.map((x) => (x.source.id === g.source.id ? { ...x, error: undefined, items: [] } : x));
    await searchAll(
      q,
      (ng) => (groups = groups.map((x) => (x.source.id === ng.source.id ? ng : x))),
      c.signal,
      [g.source.id],
    ).catch(() => {});
  }

  const found = $derived(groups.filter((g) => g.items.length));
  const empty = $derived(groups.filter((g) => !g.items.length));
</script>

<div class="view">
  <form
    class="inbox big"
    onsubmit={(e) => {
      e.preventDefault();
      run(typed);
      input?.blur();
    }}
  >
    <Search size={18} />
    <input
      class="search-input"
      bind:this={input}
      bind:value={typed}
      oninput={onInput}
      placeholder={t("search.placeholder")}
      enterkeyhint="search"
      autocomplete="off"
      spellcheck="false"
    />
    {#if searching}<LoaderCircle size={16} class="spin" />{/if}
    {#if typed}
      <button
        type="button"
        class="icon-btn"
        aria-label={t("common.clear")}
        onclick={() => {
          typed = "";
          run("");
          input?.focus();
        }}><X size={16} /></button
      >
    {/if}
  </form>

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
  .big {
    height: 48px;
    border-radius: 999px;
    padding: 0 16px;
    margin-bottom: 22px;
    max-width: 720px;
  }
  .big input {
    height: 46px;
    font-size: 16px;
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
