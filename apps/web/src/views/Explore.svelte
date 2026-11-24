<script lang="ts">
  // Explorar una fuente: pastillas con las fuentes (las fijadas primero), Populares / Ultimos y
  // buscar dentro de la fuente. Grilla con scroll infinito. La fuente y el modo viven en la URL
  // (#/explore/{sourceId}?type=latest&q=...), asi "atras" vuelve igual.
  import { Languages, PackagePlus, Pin, PinOff, RefreshCw, Search, X } from "@lucide/svelte";
  import { untrack } from "svelte";
  import CoverCard from "../components/CoverCard.svelte";
  import CoverGrid from "../components/CoverGrid.svelte";
  import Seg from "../components/Seg.svelte";
  import { api, errText } from "../lib/api";
  import { t } from "../lib/i18n.svelte";
  import { go, route, setQuery } from "../lib/router.svelte";
  import { session } from "../lib/session.svelte";
  import { isPinned, langLabel, loadSources, orderedSources, sources, togglePin, visibleSources } from "../lib/sources.svelte";
  import type { BrowseType, Source, WorkCard } from "../lib/types";

  loadSources().catch(() => {});

  // Las pastillas: las de los idiomas del usuario (Ajustes → Fuentes) y las fijadas; "+N en otros
  // idiomas" muestra el resto. La fuente de la URL vale aunque sea de otro idioma.
  let allLangs = $state(false);
  const every = $derived(sources.list ? orderedSources(sources.list) : null);
  const mine = $derived(sources.list ? visibleSources(sources.list) : null);
  const hidden = $derived((every?.length ?? 0) - (mine?.length ?? 0));
  const sourceId = $derived(route.params[0] ?? "");
  const source = $derived<Source | null>(every?.find((s) => s.id === sourceId) ?? mine?.[0] ?? every?.[0] ?? null);
  const list = $derived.by(() => {
    if (!every || !mine) return null;
    const l = allLangs ? every : mine;
    // La elegida siempre se ve (aunque sea de otro idioma).
    return source && !l.some((s) => s.id === source.id) ? [source, ...l] : l;
  });
  const q = $derived(route.query.get("q") ?? "");
  const type = $derived<BrowseType>(q ? "search" : route.query.get("type") === "latest" ? "latest" : "popular");

  let items = $state<WorkCard[] | null>(null);
  let page = $state(1);
  let hasNext = $state(false);
  let loading = $state(false);
  let error = $state("");
  let typed = $state(route.query.get("q") ?? "");
  let ctrl: AbortController | null = null;

  async function fetchPage(reset: boolean) {
    if (!source) return;
    ctrl?.abort();
    const c = (ctrl = new AbortController());
    if (reset) {
      items = null;
      page = 1;
    }
    loading = true;
    error = "";
    try {
      const r = await api.browse(source.id, type, page, q, c.signal);
      if (c.signal.aborted) return;
      // Algunas fuentes repiten titulos entre paginas: se descartan los que ya estan.
      const seen = new Set((items ?? []).map((w) => w.id));
      items = [...(items ?? []), ...r.items.filter((w) => !seen.has(w.id))];
      hasNext = r.hasNext && r.items.length > 0;
    } catch (e) {
      if ((e as Error).name === "AbortError") return;
      error = errText(e);
      if (!items) items = [];
      hasNext = false;
    } finally {
      if (ctrl === c) loading = false;
    }
  }

  // Cambia la fuente, el modo o la busqueda: de nuevo desde la pagina 1.
  $effect(() => {
    void source?.id;
    void type;
    void q;
    untrack(() => fetchPage(true));
  });

  function more() {
    if (loading || !hasNext) return;
    page++;
    fetchPage(false);
  }

  function pick(s: Source) {
    go(`/explore/${s.id}`, { replace: true });
  }

  function submit(e: Event) {
    e.preventDefault();
    setQuery({ q: typed.trim() });
  }
</script>

<div class="view">
  <div class="page-head">
    <h1>{t("explore.title")}</h1>
    <span class="grow"></span>
    {#if source}
      <button class="btn sm ghost" onclick={() => togglePin(source.id)} title={isPinned(source.id) ? t("explore.unpinHint") : t("explore.pinHint")}>
        {#if isPinned(source.id)}<PinOff size={14} />{t("explore.unpin")}{:else}<Pin size={14} />{t("explore.pin")}{/if}
      </button>
    {/if}
  </div>

  {#if every && !every.length}
    <div class="empty">
      <PackagePlus size={30} />
      <h3>{t("home.noSources")}</h3>
      {#if session.user?.admin}
        <button class="btn primary" onclick={() => go("/admin?tab=sources")}>{t("home.installSources")}</button>
      {:else}
        <p class="muted">{t("home.noSourcesUser")}</p>
      {/if}
    </div>
  {:else}
    <div class="chips-scroll sources">
      {#if list}
        {#each list as s (s.id)}
          <button class="pick" class:on={s.id === source?.id} onclick={() => pick(s)}>
            <img src={s.icon} alt="" loading="lazy" />
            <span>{s.name}</span>
            <span class="lang">{langLabel(s.lang)}</span>
            {#if isPinned(s.id)}<Pin size={12} />{/if}
            {#if s.nsfw !== "safe"}<span class="dot" title={t("explore.nsfw")}></span>{/if}
          </button>
        {/each}
        {#if hidden > 0}
          <button class="pick more-langs" onclick={() => (allLangs = !allLangs)} title={t("explore.langsHint")}>
            <Languages size={14} />
            <span>{allLangs ? t("explore.myLangs") : t("explore.allLangs", { n: hidden })}</span>
          </button>
        {/if}
      {:else}
        {#each Array(6) as _, i (i)}<span class="skel chip-skel"></span>{/each}
      {/if}
    </div>

    <div class="bar">
      <Seg
        value={type === "search" ? "popular" : type}
        options={[
          { value: "popular", label: t("explore.popular") },
          ...(source?.supportsLatest === false ? [] : [{ value: "latest", label: t("explore.latest") }]),
        ]}
        onchange={(v) => {
          typed = "";
          setQuery(v === "latest" ? { type: "latest" } : {});
        }}
      />
      <form class="inbox find" onsubmit={submit}>
        <Search size={15} />
        <input bind:value={typed} placeholder={source ? t("explore.searchIn", { source: source.name }) : ""} enterkeyhint="search" />
        {#if typed}
          <button
            type="button"
            class="icon-btn"
            aria-label={t("common.clear")}
            onclick={() => {
              typed = "";
              setQuery({});
            }}><X size={14} /></button
          >
        {/if}
      </form>
    </div>

    {#if error && !items?.length}
      <div class="empty">
        <h3>{t("explore.failed")}</h3>
        <p class="muted">{error}</p>
        <button class="btn" onclick={() => fetchPage(true)}><RefreshCw size={14} />{t("common.retry")}</button>
      </div>
    {:else if items && !items.length && !loading}
      <div class="empty"><h3>{q ? t("search.none", { q }) : t("explore.empty")}</h3></div>
    {:else}
      <CoverGrid {items} {loading} {more} hasMore={hasNext}>
        {#snippet card(w: WorkCard, i: number)}
          <CoverCard work={w} i={i % 30} />
        {/snippet}
      </CoverGrid>
      {#if error}
        <div class="more-err faint">{error} <button class="btn sm" onclick={() => fetchPage(false)}>{t("common.retry")}</button></div>
      {/if}
    {/if}
  {/if}
</div>

<style>
  .sources {
    margin-bottom: 14px;
  }
  .pick img {
    width: 18px;
    height: 18px;
    border-radius: 5px;
    object-fit: cover;
  }
  .more-langs {
    color: var(--text-3);
    border-style: dashed;
  }
  .lang {
    font-size: 10.5px;
    font-weight: 700;
    opacity: 0.6;
  }
  .dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: var(--warn);
  }
  .chip-skel {
    flex: none;
    width: 130px;
    height: 34px;
    border-radius: 999px;
  }
  .bar {
    display: flex;
    gap: 10px;
    align-items: center;
    margin-bottom: 18px;
    flex-wrap: wrap;
  }
  .find {
    flex: 1;
    min-width: 200px;
    max-width: 420px;
    height: 36px;
  }
  .find .icon-btn {
    width: 24px;
    height: 24px;
  }
  .more-err {
    display: flex;
    gap: 10px;
    align-items: center;
    justify-content: center;
    padding: 20px;
  }
</style>
