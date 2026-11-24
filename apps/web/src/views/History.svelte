<script lang="ts">
  // Historial: todo lo que se leyo, lo ultimo arriba, agrupado por dia. Tocar sigue donde quedo;
  // el menu lo saca de "Seguir leyendo" (las marcas de leido quedan).
  import { History, Info, Trash2 } from "@lucide/svelte";
  import { flip } from "svelte/animate";
  import ContinueCard from "../components/ContinueCard.svelte";
  import PopMenu, { type PopItem } from "../components/PopMenu.svelte";
  import { api } from "../lib/api";
  import { go } from "../lib/router.svelte";
  import { date, t } from "../lib/i18n.svelte";
  import { flipMs } from "../lib/motion";
  import { attempt } from "../lib/toast.svelte";
  import type { HistoryEntry } from "../lib/types";

  let items = $state<HistoryEntry[] | null>(null);
  let menu = $state<{ x: number; y: number; items: PopItem[] } | null>(null);
  api.history(200).then((h) => (items = h)).catch(() => (items = []));

  const day = (ms: number) => {
    const d = new Date(ms);
    const today = new Date();
    const y = new Date(Date.now() - 86400000);
    const same = (a: Date, b: Date) => a.toDateString() === b.toDateString();
    if (same(d, today)) return t("history.today");
    if (same(d, y)) return t("history.yesterday");
    return date(d, { weekday: "long", day: "numeric", month: "long" });
  };
  const groups = $derived.by(() => {
    const out: { label: string; items: HistoryEntry[] }[] = [];
    for (const e of items ?? []) {
      const l = day(e.progress.updatedAt);
      const g = out[out.length - 1];
      if (g && g.label === l) g.items.push(e);
      else out.push({ label: l, items: [e] });
    }
    return out;
  });

  function entryMenu(e: HistoryEntry, x: number, y: number) {
    menu = {
      x,
      y,
      items: [
        {
          label: e.work.kind === "book" ? t("book.details") : t("reader.toDetail"),
          icon: Info,
          run: () => go(e.work.kind === "book" ? `/books?edit=${encodeURIComponent(e.work.id)}` : `/manga/${e.work.id}`),
        },
        {
          label: t("home.removeContinue"),
          icon: Trash2,
          run: () =>
            attempt(async () => {
              await api.removeHistory(e.work.id);
              items = (items ?? []).filter((h) => h.work.id !== e.work.id);
            }),
        },
      ],
    };
  }
</script>

<div class="view">
  <div class="page-head"><h1>{t("nav.history")}</h1></div>
  {#if items && !items.length}
    <div class="empty"><History size={32} /><h3>{t("history.empty")}</h3></div>
  {:else if !items}
    <div class="list">
      {#each Array(6) as _, i (i)}<div class="skel ph"></div>{/each}
    </div>
  {:else}
    {#each groups as g (g.label)}
      <div class="section-title">{g.label}</div>
      <div class="list">
        {#each g.items as e, i (e.work.id)}
          <div animate:flip={{ duration: flipMs() }}>
            <ContinueCard entry={e} {i} onmenu={(x, y) => entryMenu(e, x, y)} />
          </div>
        {/each}
      </div>
    {/each}
  {/if}
</div>

{#if menu}<PopMenu x={menu.x} y={menu.y} items={menu.items} onclose={() => (menu = null)} />{/if}

<style>
  .list {
    --card-w: 100%;
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(320px, 1fr));
    gap: 12px;
  }
  .ph {
    height: 132px;
    border-radius: var(--radius);
  }
  .section-title:first-of-type {
    margin-top: 0;
  }
  @media (max-width: 759px) {
    .list {
      grid-template-columns: 1fr;
    }
  }
</style>
