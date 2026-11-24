<script lang="ts">
  // Lector de libros (#/book/{id}). Segun el formato:
  //  - CBZ: el lector de paginas con las imagenes del zip (/api/books/{id}/pages)
  //  - PDF: el lector de paginas con pdf.js (la primera vez guarda la portada y la cantidad de paginas)
  //  - EPUB, MOBI, AZW3, FB2: EbookView (foliate-js)
  //  - TXT: texto corrido con los ajustes de lectura
  // El progreso se guarda cada 3 s y al salir.
  import { onDestroy } from "svelte";
  import EbookView from "../components/EbookView.svelte";
  import ReaderShell from "../components/ReaderShell.svelte";
  import TextView from "../components/TextView.svelte";
  import { api, errText } from "../lib/api";
  import { t } from "../lib/i18n.svelte";
  import type { Segment } from "../lib/pages";
  import { back, go, route } from "../lib/router.svelte";
  import type { Book, Progress } from "../lib/types";

  const id = route.params[0] ?? "";

  let book = $state<Book | null>(null);
  let last = $state<Progress | null>(null);
  let segments = $state<Segment[]>([]);
  let error = $state("");
  let ready = $state(false);
  let destroyPdf: (() => void) | null = null;

  async function init() {
    error = "";
    try {
      const [b, p] = await Promise.all([api.book(id), api.progress(id).catch(() => ({ last: null, chapters: {} }))]);
      book = b;
      last = p.last;
      if (b.format === "cbz") {
        const r = await api.bookPages(id);
        segments = [{ id, title: "", pages: r.pages.map((src) => ({ src })) }];
      } else if (b.format === "pdf") {
        const { openPdf } = await import("../lib/pdf");
        const pdf = await openPdf(api.bookFileUrl(id));
        destroyPdf = pdf.destroy;
        segments = [{ id, title: "", pages: pdf.pages }];
        // La primera vez: la portada (primera pagina) y la cantidad de paginas para el server.
        if (b.pages !== pdf.count) api.setBookPages(id, pdf.count).catch(() => {});
        if (!b.cover)
          pdf
            .cover()
            .then((blob) => blob && api.setBookCover(id, blob))
            .catch(() => {});
      }
      ready = true;
    } catch (e) {
      error = errText(e);
    }
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
    if (keepalive)
      fetch(`/api/progress/${encodeURIComponent(id)}`, {
        method: "PUT",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(body),
        keepalive: true,
        credentials: "same-origin",
      }).catch(() => {});
    else api.saveProgress(id, body).catch(() => {});
  }
  function save(position: string, fraction: number) {
    pending = { chapter: "", position, fraction, done: fraction >= 0.995 };
    if (!timer) timer = window.setTimeout(() => flush(), 3000);
  }
  const leave = () => flush(true);
  const onHide = () => document.visibilityState === "hidden" && flush(true);
  addEventListener("pagehide", leave);
  document.addEventListener("visibilitychange", onHide);
  onDestroy(() => {
    removeEventListener("pagehide", leave);
    document.removeEventListener("visibilitychange", onHide);
    flush(true);
    destroyPdf?.();
  });

  const startPage = $derived(last && (book?.format === "pdf" || book?.format === "cbz") && !last.done ? Number(last.position) || 0 : 0);

  /** Tocar el titulo: los datos del libro (Libros, con el editor abierto si se puede editar). */
  function toDetails() {
    go(`/books?edit=${encodeURIComponent(id)}`);
  }
</script>

{#if error}
  <div class="err-screen">
    <h3>{t("reader.bookFailed")}</h3>
    <p>{error}</p>
    <div class="row">
      <button class="btn" onclick={() => back("/books")}>{t("common.back")}</button>
      <button class="btn primary" onclick={init}>{t("common.retry")}</button>
    </div>
  </div>
{:else if !book || !ready}
  <div class="loading"><div class="pulse"></div><span>{book?.title ?? ""}</span></div>
{:else if book.format === "pdf" || book.format === "cbz"}
  <ReaderShell
    title={book.title}
    {segments}
    start={{ seg: 0, page: startPage }}
    onback={() => back("/books")}
    ontitle={toDetails}
    onposition={(_s, page, count) => save(String(page), count ? (page + 1) / count : 0)}
    allowDouble
    ltr={book.format === "pdf"}
  />
{:else if book.format === "txt"}
  <TextView {book} url={api.bookFileUrl(id)} start={last?.fraction ?? 0} onback={() => back("/books")} ontitle={toDetails} onprogress={(f) => save(String(f), f)} />
{:else}
  <EbookView {book} url={api.bookFileUrl(id)} start={last?.position ?? ""} onback={() => back("/books")} ontitle={toDetails} onprogress={save} />
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
