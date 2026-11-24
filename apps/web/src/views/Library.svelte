<script lang="ts">
  // Biblioteca: pestañas por estado (con cuantos hay), favoritos, orden y la grilla con los
  // capitulos sin leer. La pestaña y el orden quedan en la URL.
  import { Check, Compass, Heart, LibraryBig, Trash2 } from "@lucide/svelte";
  import CoverCard from "../components/CoverCard.svelte";
  import CoverGrid from "../components/CoverGrid.svelte";
  import PopMenu, { type PopItem } from "../components/PopMenu.svelte";
  import Select from "../components/Select.svelte";
  import { api } from "../lib/api";
  import { t } from "../lib/i18n.svelte";
  import { go, route, setQuery } from "../lib/router.svelte";
  import { attempt } from "../lib/toast.svelte";
  import { LIBRARY_STATUSES, type LibraryEntry, type LibraryStatus } from "../lib/types";

  type Tab = LibraryStatus | "all" | "favorite";
  type Sort = "read" | "added" | "title" | "unread" | "updated";

  let entries = $state<LibraryEntry[] | null>(null);
  let menu = $state<{ x: number; y: number; items: PopItem[] } | null>(null);
  api.library().then((l) => (entries = l)).catch(() => (entries = []));

  const tab = $derived((route.query.get("tab") as Tab) || "reading");
  const sort = $derived((route.query.get("sort") as Sort) || "read");

  const counts = $derived.by(() => {
    const c: Record<string, number> = { all: 0, favorite: 0 };
    for (const e of entries ?? []) {
      c.all++;
      c[e.status] = (c[e.status] ?? 0) + 1;
      if (e.favorite) c.favorite++;
    }
    return c;
  });

  const shown = $derived.by(() => {
    if (!entries) return null;
    const l = entries.filter((e) => tab === "all" || (tab === "favorite" ? e.favorite : e.status === tab));
    const by: Record<Sort, (a: LibraryEntry, b: LibraryEntry) => number> = {
      read: (a, b) => (b.lastReadAt ?? 0) - (a.lastReadAt ?? 0),
      added: (a, b) => b.addedAt - a.addedAt,
      title: (a, b) => a.work.title.localeCompare(b.work.title),
      unread: (a, b) => (b.unread ?? 0) - (a.unread ?? 0),
      updated: (a, b) => (b.latestChapterAt ?? 0) - (a.latestChapterAt ?? 0),
    };
    return [...l].sort(by[sort]);
  });

  const tabs = $derived<{ id: Tab; label: string }[]>([
    ...LIBRARY_STATUSES.map((s) => ({ id: s as Tab, label: t(`status.${s}`) })),
    { id: "favorite", label: t("library.favorites") },
    { id: "all", label: t("library.all") },
  ]);

  function entryMenu(e: LibraryEntry, x: number, y: number) {
    menu = {
      x,
      y,
      items: [
        ...LIBRARY_STATUSES.map((s) => ({
          label: t(`status.${s}`),
          icon: e.status === s ? Check : undefined,
          run: () =>
            attempt(async () => {
              const n = await api.setLibrary(e.work.id, { status: s });
              entries = (entries ?? []).map((x) => (x.work.id === e.work.id ? n : x));
            }),
        })),
        {
          label: e.favorite ? t("manga.unfavorite") : t("manga.favorite"),
          icon: Heart,
          run: () =>
            attempt(async () => {
              const n = await api.setLibrary(e.work.id, { favorite: !e.favorite });
              entries = (entries ?? []).map((x) => (x.work.id === e.work.id ? n : x));
            }),
        },
        {
          label: t("manga.removeFromLibrary"),
          icon: Trash2,
          run: () =>
            attempt(async () => {
              await api.removeLibrary(e.work.id);
              entries = (entries ?? []).filter((x) => x.work.id !== e.work.id);
            }, t("manga.removed")),
        },
      ],
    };
  }
</script>

<div class="view">
  <div class="page-head">
    <h1>{t("nav.library")}</h1>
    <span class="grow"></span>
    <div class="sort">
      <Select
        value={sort}
        options={[
          { value: "read", label: t("library.sort.read") },
          { value: "updated", label: t("library.sort.updated") },
          { value: "unread", label: t("library.sort.unread") },
          { value: "added", label: t("library.sort.added") },
          { value: "title", label: t("library.sort.title") },
        ]}
        onchange={(v) => setQuery({ tab, sort: v })}
      />
    </div>
  </div>

  <div class="chips-scroll tabs">
    {#each tabs as tb (tb.id)}
      <button class="pick" class:on={tab === tb.id} onclick={() => setQuery({ tab: tb.id, sort })}>
        {#if tb.id === "favorite"}<Heart size={14} />{/if}
        {tb.label}
        {#if entries}<span class="n">{counts[tb.id] ?? 0}</span>{/if}
      </button>
    {/each}
  </div>

  {#if shown && !shown.length}
    <div class="empty">
      <LibraryBig size={32} />
      {#if entries && !entries.length}
        <h3>{t("library.empty")}</h3>
        <p class="muted">{t("library.emptyHint")}</p>
        <button class="btn primary" onclick={() => go("/explore")}><Compass size={15} />{t("nav.explore")}</button>
      {:else}
        <h3>{t("library.emptyTab")}</h3>
      {/if}
    </div>
  {:else}
    <CoverGrid items={shown} skeletons={12}>
      {#snippet card(e: LibraryEntry, i: number)}
        <CoverCard work={e.work} {i} unread={e.unread} sub={e.work.kind === "book" ? (e.work.format ?? "").toUpperCase() : (e.work.source ?? "")} onmenu={(x, y) => entryMenu(e, x, y)} />
      {/snippet}
    </CoverGrid>
  {/if}
</div>

{#if menu}<PopMenu x={menu.x} y={menu.y} items={menu.items} onclose={() => (menu = null)} />{/if}

<style>
  .sort {
    width: 200px;
  }
  .tabs {
    margin-bottom: 20px;
  }
  .n {
    font-size: 11px;
    font-weight: 700;
    opacity: 0.65;
    font-variant-numeric: tabular-nums;
  }
  .empty p {
    margin: 0;
  }
  @media (max-width: 759px) {
    .sort {
      width: 160px;
    }
  }
</style>
