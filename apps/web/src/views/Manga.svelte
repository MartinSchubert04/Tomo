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
  import Img from "../components/Img.svelte";
  import PopMenu, { type PopItem } from "../components/PopMenu.svelte";
  import { api, errText } from "../lib/api";
  import { chapterShort } from "../lib/format";
  import { t } from "../lib/i18n.svelte";
  import { expand } from "../lib/motion";
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

  // A donde lleva "Leer": donde quedo (o el siguiente si lo termino), o el primer capitulo.
  const resume = $derived.by((): { chapter: Chapter; page: string; label: string } | null => {
    if (!manga?.chapters.length) return null;
    const chs = manga.chapters;
    const last = progress.last;
    if (last?.chapter) {
      const i = chs.findIndex((c) => c.id === last.chapter);
      if (i >= 0) {
        if (last.done && i > 0) return { chapter: chs[i - 1], page: "0", label: t("manga.continueAt", { ch: chapterShort(chs[i - 1]) }) };
        if (!last.done) return { chapter: chs[i], page: last.position, label: t("manga.continueAt", { ch: chapterShort(chs[i]) }) };
      }
    }
    const first = chs[chs.length - 1];
    return { chapter: first, page: "0", label: t("manga.startReading") };
  });

  function open(c: Chapter, page = "0") {
    go(`/read/${id}/${c.id}` + (page !== "0" ? `?page=${encodeURIComponent(page)}` : ""));
  }

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
    <div class="view top">
      <div class="cover">
        {#if manga}
          <Img src={manga.cover} eager fallback={manga.title} vt={`cover-${id}`} />
        {:else}
          <div class="skel fill" style="view-transition-name: cover-{id}"></div>
        {/if}
      </div>
      <div class="info">
        {#if manga}
          <h1 class="display-title">{manga.title}</h1>
          <div class="by muted">
            {[manga.author, manga.artist && manga.artist !== manga.author ? manga.artist : null].filter(Boolean).join(" · ") || t("manga.unknownAuthor")}
          </div>
          <div class="chips">
            <span class="chip" class:ok={manga.status === "completed"} class:accent={manga.status === "ongoing"}>{statusLabel}</span>
            {#if manga.source}<span class="chip">{manga.source}</span>{/if}
            <span class="chip">{t("manga.chapters", { n: manga.chapters.length })}</span>
          </div>
          {#if manga.genres.length}
            <div class="genres">
              {#each manga.genres as g (g)}<span class="genre">{g}</span>{/each}
            </div>
          {/if}
          <div class="actions">
            {#if resume}
              <button class="btn primary lg" onclick={() => open(resume.chapter, resume.page)}>
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
          <ChapterList chapters={manga.chapters} progress={progress.chapters} current={progress.last?.chapter ?? ""} onopen={(c) => open(c, progress.chapters[c.id] && !progress.chapters[c.id].done ? progress.chapters[c.id].position : "0")} onmark={mark} />
        </div>
      {:else}
        {#each Array(8) as _, i (i)}
          <div class="ch-skel"><span class="skel line" style="width: {30 + ((i * 13) % 30)}%"></span><span class="skel line" style="width: 18%; height: 9px"></span></div>
        {/each}
      {/if}
    </div>
  {/if}
</div>

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
    max-width: 980px;
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
