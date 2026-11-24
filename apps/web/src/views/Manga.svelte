<script lang="ts">
  // Detalle de un manga: la portada desenfocada de fondo, portada, titulo, autor, estado, generos,
  // sinopsis, "Leer" / "Seguir en el cap. N", biblioteca (con estado), favorito, el sitio original
  // y la lista de capitulos.
  import {
    BookOpenText,
    Check,
    ChevronDown,
    ExternalLink,
    Heart,
    LibraryBig,
    Plus,
    RefreshCw,
    Trash2,
  } from "@lucide/svelte";
  import ChapterList from "../components/ChapterList.svelte";
  import { groupChapters, groupOf, pickVariant, preferredScan, setPreferredScan } from "../lib/chapters";
  import Img from "../components/Img.svelte";
  import PopMenu, { type PopItem } from "../components/PopMenu.svelte";
  import { api, errText } from "../lib/api";
  import { chapterShort } from "../lib/format";
  import { t } from "../lib/i18n.svelte";
  import { expand, fadeQuick, pop } from "../lib/motion";
  import { back, go, route } from "../lib/router.svelte";
  import { attempt, toast } from "../lib/toast.svelte";
  import { LIBRARY_STATUSES, type Chapter, type LibraryEntry, type LibraryStatus, type MangaDetail, type WorkProgress } from "../lib/types";

  const id = route.params[0] ?? "";

  let manga = $state<MangaDetail | null>(null);
  let error = $state("");
  let progress = $state<WorkProgress>({ last: null, chapters: {} });
  let entry = $state<LibraryEntry | null>(null);
  let refreshing = $state(false);
  let more = $state(false);
  let menu = $state<{ x: number; y: number; items: PopItem[] } | null>(null);
  // La portada en grande (tocar la portada).
  let zoomCover = $state(false);

  async function load(refresh = false) {
    error = "";
    try {
      manga = await api.manga(id, refresh);
    } catch (e) {
      error = errText(e);
    }
  }
  load();
  api.progress(id).then((p) => (progress = p)).catch(() => {});
  api.library().then((l) => (entry = l.find((e) => e.work.id === id) ?? null)).catch(() => {});

  async function refresh() {
    refreshing = true;
    await load(true);
    refreshing = false;
  }

  // A donde lleva "Leer": donde quedo (o el numero siguiente si lo termino, en el mismo idioma y
  // scan si esta), o el primer capitulo. Por numero, no por version (lib/chapters.ts).
  const groups = $derived(manga ? groupChapters(manga.chapters) : []);
  const resume = $derived.by((): { chapter: Chapter; page: string; label: string } | null => {
    if (!groups.length) return null;
    const last = progress.last;
    const preferScan = preferredScan(id);
    if (last?.chapter) {
      const gi = groupOf(groups, last.chapter);
      const lastCh = gi >= 0 ? groups[gi].variants.find((v) => v.id === last.chapter) : undefined;
      if (lastCh) {
        if (last.done && gi > 0) {
          const next = pickVariant(groups[gi - 1], { lang: lastCh.lang, scan: lastCh.scanlator, preferScan });
          return { chapter: next, page: "0", label: t("manga.continueAt", { ch: chapterShort(next) }) };
        }
        if (!last.done) return { chapter: lastCh, page: last.position, label: t("manga.continueAt", { ch: chapterShort(lastCh) }) };
      }
    }
    const first = pickVariant(groups[groups.length - 1], { preferScan });
    return { chapter: first, page: "0", label: t("manga.startReading") };
  });

  function open(c: Chapter, page = "0") {
    // El scan que se elige queda como preferido de este manga (el siguiente sigue con ese).
    if (c.scanlator) setPreferredScan(id, c.scanlator);
    go(`/read/${id}/${c.id}` + (page !== "0" ? `?page=${encodeURIComponent(page)}` : ""));
  }

  const authors = $derived(manga ? [manga.author, manga.artist && manga.artist !== manga.author ? manga.artist : null].filter((a): a is string => !!a) : []);

  async function setStatus(status: LibraryStatus) {
    await attempt(async () => {
      entry = await api.setLibrary(id, { status });
    }, entry ? undefined : t("manga.added"));
  }

  function libraryMenu(e: MouseEvent) {
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    const items: PopItem[] = LIBRARY_STATUSES.map((s) => ({
      label: t(`status.${s}`),
      icon: entry?.status === s ? Check : undefined,
      run: () => setStatus(s),
    }));
    if (entry)
      items.push({
        label: t("manga.removeFromLibrary"),
        icon: Trash2,
        run: () =>
          attempt(async () => {
            await api.removeLibrary(id);
            entry = null;
          }, t("manga.removed")),
      });
    menu = { x: r.left, y: r.bottom + 6, items };
  }

  async function toggleFav() {
    const fav = !entry?.favorite;
    await attempt(async () => {
      entry = await api.setLibrary(id, { favorite: fav });
    });
  }

  async function mark(ids: string[], done: boolean) {
    const before = progress.chapters;
    // Al toque en la pantalla; si falla, vuelve.
    const next = { ...before };
    const now = Date.now();
    for (const c of ids) next[c] = done ? { chapter: c, position: "0", fraction: 1, done: true, updatedAt: now } : { ...(next[c] ?? { chapter: c, position: "0", fraction: 0, updatedAt: now }), done: false, fraction: 0 };
    progress = { ...progress, chapters: next };
    try {
      await api.mark(id, ids, done);
    } catch (e) {
      progress = { ...progress, chapters: before };
      toast(errText(e), true);
    }
  }

  const statusLabel = $derived(manga ? t(`mstatus.${manga.status}`) : "");
</script>

<div class="detail">
  {#if manga?.cover}
    <div class="backdrop" aria-hidden="true"><img src={manga.cover} alt="" /></div>
  {/if}

  {#if error && !manga}
    <div class="view">
      <div class="empty">
        <h3>{t("manga.failed")}</h3>
        <p class="muted">{error}</p>
        <div class="row">
          <button class="btn" onclick={() => back("/explore")}>{t("common.back")}</button>
          <button class="btn primary" onclick={() => load()}><RefreshCw size={14} />{t("common.retry")}</button>
        </div>
      </div>
    </div>
  {:else}
    <div class="layout">
    <div class="view top">
      <div class="cover">
        {#if manga}
          <button class="cover-btn" onclick={() => manga?.cover && (zoomCover = true)} title={t("manga.viewCover")} aria-label={t("manga.viewCover")} disabled={!manga.cover}>
            <Img src={manga.cover} eager fallback={manga.title} vt={`cover-${id}`} />
          </button>
        {:else}
          <div class="skel fill" style="view-transition-name: cover-{id}"></div>
        {/if}
      </div>
      <div class="info">
        {#if manga}
          <h1 class="display-title">{manga.title}</h1>
          {#if authors.length}
            <div class="by muted">
              {#each authors as a, i (a)}{#if i > 0}{" · "}{/if}<button class="link-text" onclick={() => go(`/search?q=${encodeURIComponent(a)}`)} title={t("manga.searchAuthor", { name: a })}>{a}</button>{/each}
            </div>
          {/if}
          <div class="chips">
            {#if manga.status !== "unknown"}
              <span class="chip" class:ok={manga.status === "completed"} class:accent={manga.status === "ongoing"}>{statusLabel}</span>
            {/if}
            {#if manga.source}
              <button class="chip click" onclick={() => go(`/explore/${manga?.sourceId}`)} title={t("manga.exploreSource", { source: manga.source })}>{manga.source}</button>
            {/if}
            <span class="chip">{t("manga.chapters", { n: groups.length })}</span>
          </div>
          {#if manga.genres.length}
            <div class="genres">
              {#each manga.genres as g (g)}
                <button class="genre" onclick={() => go(`/explore/${manga?.sourceId}?q=${encodeURIComponent(g)}`)} title={t("manga.searchGenre", { genre: g, source: manga.source ?? "" })}>{g}</button>
              {/each}
            </div>
          {/if}
          <div class="actions">
            {#if resume}
              <button class="btn primary lg" onclick={() => open(resume.chapter, resume.page)} title={t("keys.read") + " (R)"}>
                <BookOpenText size={18} /><span class="ellipsis">{resume.label}</span>
              </button>
            {/if}
            <button class="btn lg" class:in={!!entry} onclick={libraryMenu}>
              {#if entry}<LibraryBig size={17} />{t(`status.${entry.status}`)}{:else}<Plus size={17} />{t("manga.addToLibrary")}{/if}
              <ChevronDown size={15} />
            </button>
            <button class="btn lg icon" class:fav={entry?.favorite} onclick={toggleFav} title={entry?.favorite ? t("manga.unfavorite") : t("manga.favorite")}>
              <Heart size={18} />
            </button>
            {#if manga.url}
              <a class="btn lg icon" href={manga.url} target="_blank" rel="noreferrer noopener" title={t("manga.openSite")}><ExternalLink size={17} /></a>
            {/if}
            <button class="btn lg icon" onclick={refresh} disabled={refreshing} title={t("manga.refresh")}>
              <RefreshCw size={17} class={refreshing ? "spin" : ""} />
            </button>
          </div>
          {#if manga.description}
            <div class="desc" class:open={more}>
              <p>{manga.description}</p>
            </div>
            {#if manga.description.length > 280}
              <button class="link" onclick={() => (more = !more)}>{more ? t("common.less") : t("common.more")}</button>
            {/if}
          {/if}
        {:else}
          <div class="skel line" style="width: 70%; height: 30px"></div>
          <div class="skel line" style="width: 35%; margin-top: 12px"></div>
          <div class="row" style="margin-top: 16px"><span class="skel line" style="width: 90px; height: 22px"></span><span class="skel line" style="width: 120px; height: 22px"></span></div>
          <div class="row" style="margin-top: 22px"><span class="skel" style="width: 190px; height: 42px"></span><span class="skel" style="width: 150px; height: 42px"></span></div>
          <div class="stack" style="margin-top: 22px; gap: 8px">
            <span class="skel line" style="width: 100%"></span><span class="skel line" style="width: 96%"></span><span class="skel line" style="width: 80%"></span>
          </div>
        {/if}
      </div>
    </div>

    <div class="view chapters">
      {#if manga}
        <div transition:expand>
          <ChapterList work={id} chapters={manga.chapters} progress={progress.chapters} current={progress.last?.chapter ?? ""} onopen={(c) => open(c, progress.chapters[c.id] && !progress.chapters[c.id].done ? progress.chapters[c.id].position : "0")} onmark={mark} />
        </div>
      {:else}
        {#each Array(8) as _, i (i)}
          <div class="ch-skel"><span class="skel line" style="width: {30 + ((i * 13) % 30)}%"></span><span class="skel line" style="width: 18%; height: 9px"></span></div>
        {/each}
      {/if}
    </div>
    </div>
  {/if}
</div>

{#if zoomCover && manga?.cover}
  <div class="lightbox" role="presentation" onclick={() => (zoomCover = false)} transition:fadeQuick>
    <img src={manga.cover} alt={manga.title} in:pop={{ start: 0.92 }} />
  </div>
{/if}
<svelte:window
  onkeydown={(e) => {
    if (e.key === "Escape" && zoomCover) return (zoomCover = false), e.preventDefault();
    // R: leer / seguir leyendo (lib/shortcuts.ts).
    const typing = (e.target as HTMLElement)?.closest?.("input, textarea, [contenteditable]");
    if (!typing && !e.ctrlKey && !e.metaKey && !e.altKey && (e.key === "r" || e.key === "R") && resume) {
      e.preventDefault();
      open(resume.chapter, resume.page);
    }
  }}
/>

{#if menu}<PopMenu x={menu.x} y={menu.y} items={menu.items} onclose={() => (menu = null)} />{/if}

<style>
  .detail {
    position: relative;
    isolation: isolate;
  }
  .backdrop {
    position: absolute;
    left: 0;
    right: 0;
    top: 0;
    height: 520px;
    z-index: -1;
    overflow: hidden;
    opacity: 0.45;
    animation: fadein 0.6s ease backwards;
  }
  .backdrop img {
    width: 100%;
    height: 100%;
    object-fit: cover;
    filter: blur(40px) saturate(1.4);
    transform: scale(1.15);
  }
  .backdrop::after {
    content: "";
    position: absolute;
    inset: 0;
    background: linear-gradient(180deg, transparent 0%, color-mix(in srgb, var(--bg) 70%, transparent) 55%, var(--bg) 100%);
  }
  :global([data-lite]) .backdrop img {
    filter: none;
    opacity: 0.4;
  }
  @keyframes fadein {
    from {
      opacity: 0;
    }
  }
  .top {
    display: grid;
    grid-template-columns: 220px 1fr;
    gap: 32px;
    padding-bottom: 12px;
  }
  .cover {
    position: relative;
    aspect-ratio: 2 / 3;
    border-radius: var(--radius);
    overflow: hidden;
    background: var(--panel-2);
    border: var(--border) solid var(--line);
    box-shadow: 0 24px 50px -24px var(--media-scrim), var(--card-shadow);
    align-self: start;
  }
  .fill {
    position: absolute;
    inset: 0;
    border-radius: 0;
  }
  .info {
    min-width: 0;
    display: flex;
    flex-direction: column;
    padding-top: 6px;
  }
  .display-title {
    font-size: clamp(24px, 3.6vw, 42px);
  }
  .by {
    margin-top: 8px;
    font-size: 14px;
  }
  .chips {
    display: flex;
    gap: 6px;
    flex-wrap: wrap;
    margin-top: 14px;
  }
  .chips .chip {
    height: 24px;
    padding: 0 10px;
    font-size: 12px;
  }
  .genres {
    display: flex;
    gap: 6px;
    flex-wrap: wrap;
    margin-top: 10px;
  }
  .genre {
    font-size: 12px;
    color: var(--text-2);
    padding: 3px 9px;
    border-radius: var(--radius-sm);
    background: color-mix(in srgb, var(--panel-2) 80%, transparent);
    border: var(--border) solid transparent;
    transition:
      color 0.15s,
      border-color 0.15s,
      transform 0.15s;
  }
  .genre:hover {
    color: var(--text);
    border-color: var(--line-strong);
    transform: translateY(-1px);
  }
  .chip.click {
    cursor: pointer;
    transition: border-color 0.15s;
  }
  .chip.click:hover {
    border-color: var(--accent);
  }
  .link-text {
    color: inherit;
    text-decoration: underline;
    text-decoration-color: transparent;
    text-underline-offset: 3px;
    transition: text-decoration-color 0.15s, color 0.15s;
  }
  .link-text:hover {
    color: var(--text);
    text-decoration-color: currentColor;
  }
  .cover-btn {
    position: absolute;
    inset: 0;
    cursor: zoom-in;
  }
  .lightbox {
    position: fixed;
    inset: 0;
    z-index: 80;
    display: grid;
    place-items: center;
    padding: 24px;
    background: color-mix(in srgb, var(--media-bg) 82%, transparent);
    backdrop-filter: blur(10px);
    cursor: zoom-out;
  }
  .lightbox img {
    max-width: 100%;
    max-height: calc(var(--vh) - 48px);
    border-radius: var(--radius);
    box-shadow: var(--pop-shadow);
  }
  .actions {
    display: flex;
    gap: 8px;
    margin-top: 20px;
    flex-wrap: wrap;
  }
  .actions .icon {
    width: 42px;
    padding: 0;
  }
  .actions .in {
    border-color: color-mix(in srgb, var(--accent) 45%, var(--line));
  }
  .actions .in :global(svg:first-child) {
    color: var(--accent);
  }
  .fav {
    color: var(--danger);
  }
  .fav :global(svg) {
    fill: currentColor;
    animation: beat 0.35s cubic-bezier(0.34, 1.56, 0.64, 1);
  }
  @keyframes beat {
    50% {
      transform: scale(1.3);
    }
  }
  .primary {
    max-width: 100%;
    min-width: 0;
  }
  .desc {
    margin-top: 18px;
    max-width: 780px;
    font-size: 14.5px;
    line-height: 1.6;
    color: var(--text-2);
    display: -webkit-box;
    -webkit-line-clamp: 4;
    line-clamp: 4;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }
  .desc.open {
    display: block;
  }
  .desc p {
    margin: 0;
    white-space: pre-line;
  }
  .link {
    align-self: flex-start;
    margin-top: 4px;
    color: var(--accent);
    font-weight: 600;
    font-size: 13px;
  }
  .chapters {
    padding-top: 8px;
  }
  /* Pantallas anchas (ancho util ≥ 1400 px CSS, ver lib/scale.svelte.ts): dos columnas, la ficha
     fija a la izquierda y los capitulos llenando la derecha. */
  @container page (min-width: 1400px) {
    .layout {
      display: grid;
      grid-template-columns: minmax(440px, 0.85fr) 1.15fr;
      align-items: start;
      max-width: var(--page-max);
      margin: 0 auto;
    }
    .top {
      position: sticky;
      top: 0;
      max-height: calc(var(--vh) - 70px);
      overflow-y: auto;
      grid-template-columns: 180px 1fr;
      gap: 24px;
      padding-right: 12px;
      scrollbar-width: none;
    }
    .chapters {
      max-width: none;
      padding-top: 22px;
    }
  }
  .ch-skel {
    display: flex;
    flex-direction: column;
    justify-content: center;
    gap: 8px;
    height: 56px;
    padding: 0 12px;
  }
  @media (max-width: 759px) {
    .top {
      grid-template-columns: 1fr;
      gap: 18px;
      text-align: center;
    }
    .cover {
      width: 150px;
      justify-self: center;
    }
    .chips,
    .genres,
    .actions {
      justify-content: center;
    }
    .actions .primary {
      flex: 1 1 100%;
    }
    .desc {
      text-align: left;
    }
    .link {
      align-self: center;
    }
    .backdrop {
      height: 380px;
    }
  }
</style>
