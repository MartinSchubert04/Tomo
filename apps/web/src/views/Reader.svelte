<script lang="ts">
  // Lector de manga (#/read/{workId}/{chapterId}?page=). Arma los "segmentos" (capitulos) para
  // ReaderShell: en cascada, al acercarse al final se agrega el capitulo siguiente abajo y la URL
  // sigue al capitulo que se esta leyendo. Guarda el progreso cada 3 s y al salir; un capitulo
  // cuenta como leido al llegar a la ultima pagina.
  import { onDestroy } from "svelte";
  import ReaderShell, { type PickItem } from "../components/ReaderShell.svelte";
  import { api, errText } from "../lib/api";
  import { chapterShort, chapterTitle } from "../lib/format";
  import { ago, t } from "../lib/i18n.svelte";
  import type { Segment } from "../lib/pages";
  import { mangaPrefs } from "../lib/readerPrefs.svelte";
  import { back, go, route } from "../lib/router.svelte";
  import type { Chapter, MangaDetail, Progress, WorkProgress } from "../lib/types";

  const workId = route.params[0] ?? "";

  let manga = $state<MangaDetail | null>(null);
  let progress = $state<WorkProgress>({ last: null, chapters: {} });
  let segments = $state<Segment[]>([]);
  let start = $state({ seg: 0, page: 0 });
  let session = $state(0);
  let error = $state("");
  let cur = $state(0);
  let loadingNext = $state(false);

  const chapters = $derived(manga?.chapters ?? []);
  const curId = $derived(segments[cur]?.id ?? route.params[1] ?? "");
  const idx = $derived(chapters.findIndex((c) => c.id === curId));
  // La lista viene del mas nuevo al mas viejo: el siguiente es el de antes en la lista.
  const nextCh = $derived<Chapter | null>(idx > 0 ? chapters[idx - 1] : null);
  const prevCh = $derived<Chapter | null>(idx >= 0 && idx < chapters.length - 1 ? chapters[idx + 1] : null);

  const pickItems = $derived<PickItem[]>(
    chapters.map((c) => ({
      id: c.id,
      label: chapterShort(c) + (chapterTitle(c) ? ` · ${chapterTitle(c)}` : ""),
      sub: [c.scanlator, c.date ? ago(c.date) : ""].filter(Boolean).join(" · "),
      done: !!progress.chapters[c.id]?.done,
    })),
  );

  async function segmentFor(c: Chapter): Promise<Segment> {
    const r = await api.pages(workId, c.id);
    return { id: c.id, title: chapterShort(c), pages: r.pages.map((src) => ({ src })) };
  }

  /** Empieza (o vuelve a empezar) a leer en un capitulo y una pagina. */
  async function openChapter(chapterId: string, page: number | null) {
    error = "";
    flush();
    const c = chapters.find((x) => x.id === chapterId) ?? ({ id: chapterId, name: "", number: -1, scanlator: null, date: 0 } as Chapter);
    try {
      const s = await segmentFor(c);
      const saved = progress.chapters[chapterId];
      const p = page ?? (saved && !saved.done ? Number(saved.position) || 0 : 0);
      segments = [s];
      start = { seg: 0, page: Math.min(Math.max(0, p), Math.max(0, s.pages.length - 1)) };
      cur = 0;
      session++;
      if (route.params[1] !== chapterId) go(`/read/${workId}/${chapterId}`, { replace: true });
    } catch (e) {
      error = errText(e);
    }
  }

  async function init() {
    const chapterId = route.params[1] ?? "";
    const qp = route.query.get("page");
    try {
      const [m, pr] = await Promise.all([api.manga(workId), api.progress(workId).catch(() => ({ last: null, chapters: {} }) as WorkProgress)]);
      manga = m;
      progress = pr;
    } catch (e) {
      error = errText(e);
      return;
    }
    await openChapter(chapterId, qp !== null ? Number(qp) : null);
  }
  init();

  // ---------- progreso ----------
  let pending: Omit<Progress, "updatedAt"> | null = null;
  let timer = 0;

  function flush(keepalive = false) {
    clearTimeout(timer);
    timer = 0;
    if (!pending) return;
    const body = pending;
    pending = null;
    if (keepalive) {
      fetch(`/api/progress/${encodeURIComponent(workId)}`, {
        method: "PUT",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(body),
        keepalive: true,
        credentials: "same-origin",
      }).catch(() => {});
    } else api.saveProgress(workId, body).catch(() => {});
  }

  function onposition(s: number, page: number, count: number) {
    cur = s;
    const chapter = segments[s]?.id;
    if (!chapter) return;
    const done = count > 0 && page >= count - 1;
    const prev = progress.chapters[chapter];
    const p = { chapter, position: String(page), fraction: count ? (page + 1) / count : 0, done: done || !!prev?.done };
    progress.chapters[chapter] = { ...p, updatedAt: Date.now() };
    pending = p;
    if (done && !prev?.done) flush();
    else if (!timer) timer = window.setTimeout(() => flush(), 3000);
    // La URL sigue al capitulo que se lee (en cascada, al pasar al agregado).
    if (route.params[1] !== chapter) go(`/read/${workId}/${chapter}`, { replace: true });
  }

  async function onnearend() {
    if (!mangaPrefs.autoNext || loadingNext) return;
    const lastId = segments[segments.length - 1]?.id;
    const i = chapters.findIndex((c) => c.id === lastId);
    const n = i > 0 ? chapters[i - 1] : null;
    if (!n || segments.some((s) => s.id === n.id)) return;
    loadingNext = true;
    try {
      segments = [...segments, await segmentFor(n)];
    } catch {
      // Sin el siguiente queda la tarjeta del final con el boton.
    } finally {
      loadingNext = false;
    }
  }

  function leave() {
    flush(true);
  }
  const onHide = () => document.visibilityState === "hidden" && flush(true);
  addEventListener("pagehide", leave);
  document.addEventListener("visibilitychange", onHide);
  onDestroy(() => {
    removeEventListener("pagehide", leave);
    document.removeEventListener("visibilitychange", onHide);
    flush(true);
  });
</script>

{#if error}
  <div class="err-screen">
    <h3>{t("reader.failed")}</h3>
    <p>{error}</p>
    <div class="row">
      <button class="btn" onclick={() => back(`/manga/${workId}`)}>{t("common.back")}</button>
      <button class="btn primary" onclick={() => (manga ? openChapter(curId, null) : init())}>{t("common.retry")}</button>
    </div>
  </div>
{:else if !segments.length}
  <div class="loading">
    <div class="pulse"></div>
    <span>{manga?.title ?? ""}</span>
  </div>
{:else}
  {#key session}
    <ReaderShell
      title={manga?.title ?? ""}
      {segments}
      {start}
      onback={() => back(`/manga/${workId}`)}
      {onposition}
      {onnearend}
      hasPrev={!!prevCh}
      hasNext={!!nextCh}
      {loadingNext}
      nextLabel={nextCh ? chapterShort(nextCh) : ""}
      onprev={() => prevCh && openChapter(prevCh.id, 0)}
      onnext={() => nextCh && openChapter(nextCh.id, 0)}
      chapters={pickItems}
      currentChapter={curId}
      onpick={(id) => openChapter(id, null)}
    />
  {/key}
{/if}

<style>
  .err-screen,
  .loading {
    position: fixed;
    inset: 0;
    z-index: 20;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 12px;
    padding: 24px;
    text-align: center;
    background: var(--media-bg);
    color: var(--on-media);
  }
  .err-screen p {
    margin: 0;
    color: var(--on-media-2);
    max-width: 420px;
  }
  .err-screen h3 {
    margin: 0;
  }
  .loading span {
    color: var(--on-media-2);
    font-size: 13px;
  }
  .pulse {
    width: 46px;
    height: 64px;
    border-radius: 6px;
    background: color-mix(in srgb, var(--accent) 70%, transparent);
    animation: breathe 1.2s ease-in-out infinite;
  }
  @keyframes breathe {
    50% {
      transform: scale(0.88);
      opacity: 0.5;
    }
  }
</style>
