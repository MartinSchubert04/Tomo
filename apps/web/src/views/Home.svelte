<script lang="ts">
  // Inicio: seguir leyendo, capitulos nuevos de la biblioteca y, de la fuente principal del
  // usuario, populares y ultimos. Sin fuentes instaladas: el aviso (al admin, con el boton).
  import { Compass, PackagePlus, Sparkles, Trash2 } from "@lucide/svelte";
  import ContinueCard from "../components/ContinueCard.svelte";
  import CoverCard from "../components/CoverCard.svelte";
  import PopMenu, { type PopItem } from "../components/PopMenu.svelte";
  import Row from "../components/Row.svelte";
  import { api } from "../lib/api";
  import { t } from "../lib/i18n.svelte";
  import { go } from "../lib/router.svelte";
  import { session } from "../lib/session.svelte";
  import { loadSources, mainSource, sources } from "../lib/sources.svelte";
  import { attempt } from "../lib/toast.svelte";
  import type { HistoryEntry, LibraryEntry, WorkCard } from "../lib/types";

  let history = $state<HistoryEntry[] | null>(null);
  let library = $state<LibraryEntry[] | null>(null);
  let menu = $state<{ x: number; y: number; items: PopItem[] } | null>(null);

  api.history(20).then((h) => (history = h)).catch(() => (history = []));
  api.library().then((l) => (library = l)).catch(() => (library = []));
  loadSources().catch(() => {});

  const updates = $derived(
    (library ?? [])
      .filter((e) => e.work.kind === "manga" && (e.unread ?? 0) > 0)
      .sort((a, b) => (b.latestChapterAt ?? 0) - (a.latestChapterAt ?? 0)),
  );
  const main = $derived(sources.list ? mainSource(sources.list) : null);

  function historyMenu(e: HistoryEntry, x: number, y: number) {
    menu = {
      x,
      y,
      items: [
        {
          label: t("home.removeContinue"),
          icon: Trash2,
          run: () =>
            attempt(async () => {
              await api.removeHistory(e.work.id);
              history = (history ?? []).filter((h) => h.work.id !== e.work.id);
            }),
        },
      ],
    };
  }
</script>

<div class="home">
  <header class="hello">
    <h1>{t("home.hello", { name: session.user?.name ?? "" })}</h1>
  </header>

  {#if history && history.length}
    <Row title={t("home.continue")} items={history} wide>
      {#snippet card(e: HistoryEntry, i: number)}
        <ContinueCard entry={e} {i} onmenu={(x, y) => historyMenu(e, x, y)} />
      {/snippet}
    </Row>
  {/if}

  {#if updates.length}
    <Row title={t("home.updates")} items={updates} href="/library">
      {#snippet card(e: LibraryEntry, i: number)}
        <CoverCard work={e.work} {i} unread={e.unread} sub={e.work.source ?? ""} />
      {/snippet}
    </Row>
  {/if}

  {#if sources.list && !sources.list.length}
    <div class="empty card nosrc">
      <PackagePlus size={30} />
      <h3>{t("home.noSources")}</h3>
      {#if session.user?.admin}
        <p class="muted">{t("home.noSourcesAdmin")}</p>
        <button class="btn primary" onclick={() => go("/admin?tab=sources")}><PackagePlus size={15} />{t("home.installSources")}</button>
      {:else}
        <p class="muted">{t("home.noSourcesUser")}</p>
      {/if}
    </div>
  {:else if main}
    {#key main.id}
      <Row
        title={t("home.popularIn", { source: main.name })}
        load={() => api.browse(main.id, "popular", 1).then((p) => p.items)}
        href={`/explore/${main.id}`}
      >
        {#snippet card(w: WorkCard, i: number)}
          <CoverCard work={w} {i} />
        {/snippet}
      </Row>
      {#if main.supportsLatest}
        <Row
          title={t("home.latestIn", { source: main.name })}
          load={() => api.browse(main.id, "latest", 1).then((p) => p.items)}
          href={`/explore/${main.id}?type=latest`}
        >
          {#snippet card(w: WorkCard, i: number)}
            <CoverCard work={w} {i} />
          {/snippet}
        </Row>
      {/if}
    {/key}
    <div class="explore-more">
      <button class="btn lg" onclick={() => go("/explore")}><Compass size={17} />{t("home.exploreMore")}</button>
    </div>
  {:else if !sources.list}
    <Row title={t("home.popular")} load={() => new Promise<WorkCard[]>(() => {})}>
      {#snippet card(w: WorkCard, i: number)}
        <CoverCard work={w} {i} />
      {/snippet}
    </Row>
  {/if}

  {#if history && !history.length && library && !library.length && sources.list?.length}
    <div class="first faint"><Sparkles size={15} />{t("home.firstTime")}</div>
  {/if}
</div>

{#if menu}<PopMenu x={menu.x} y={menu.y} items={menu.items} onclose={() => (menu = null)} />{/if}

<style>
  .home {
    padding: 8px 0 40px;
  }
  @media (max-width: 759px) {
    .home {
      padding-bottom: calc(24px + var(--tabbar-h) + env(safe-area-inset-bottom));
    }
  }
  .hello {
    padding: 0 var(--gutter);
    margin: 4px 0 18px;
  }
  h1 {
    font-family: var(--display);
    font-size: clamp(22px, 3vw, 30px);
    font-weight: 760;
    letter-spacing: -0.03em;
    margin: 0;
  }
  .nosrc {
    margin: 10px var(--gutter);
    padding: 40px 20px;
  }
  .nosrc p {
    margin: 0;
    max-width: 420px;
  }
  .explore-more {
    display: flex;
    justify-content: center;
    padding: 8px var(--gutter) 0;
  }
  .first {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 8px;
    padding: 20px var(--gutter);
    font-size: 13px;
  }
</style>
