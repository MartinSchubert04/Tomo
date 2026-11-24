<script lang="ts">
  // Libros: la estanteria de todo lo subido (compartido entre usuarios), con filtro por formato y
  // por texto. Subir: el boton o soltar archivos en cualquier parte de la vista (aparece un marco
  // "Solta para subir"). Cada subida es una tarjeta con su barra que se convierte en el libro.
  import { BookUp, FileUp, LoaderCircle, Pencil, Search, Trash2, X } from "@lucide/svelte";
  import CoverCard from "../components/CoverCard.svelte";
  import CoverGrid from "../components/CoverGrid.svelte";
  import Drawer from "../components/Drawer.svelte";
  import PopMenu, { type PopItem } from "../components/PopMenu.svelte";
  import { route, setQuery } from "../lib/router.svelte";
  import { api, errText, uploadBooks } from "../lib/api";
  import { ask } from "../lib/confirm.svelte";
  import { bytes } from "../lib/format";
  import { t } from "../lib/i18n.svelte";
  import { fadeQuick, rise } from "../lib/motion";
  import { session } from "../lib/session.svelte";
  import { attempt, toast } from "../lib/toast.svelte";
  import type { Book } from "../lib/types";

  const ACCEPT = ".epub,.pdf,.cbz,.zip,.mobi,.azw3,.azw,.fb2,.txt";
  const EXT = /\.(epub|pdf|cbz|zip|mobi|azw3?|fb2|txt)$/i;

  type Up = { id: number; name: string; size: number; frac: number; error: string; abort: () => void };

  let books = $state<Book[] | null>(null);
  let progress = $state<Record<string, number>>({});
  let uploads = $state<Up[]>([]);
  let filter = $state<"all" | "epub" | "pdf" | "cbz" | "other">("all");
  let q = $state("");
  let over = $state(false);
  let picker = $state<HTMLInputElement>();
  let editing = $state<Book | null>(null);
  let editTitle = $state("");
  let editAuthor = $state("");
  let menu = $state<{ x: number; y: number; items: PopItem[] } | null>(null);
  let nextId = 1;

  api.books().then((b) => (books = b)).catch((e) => ((books = []), toast(errText(e), true)));
  // #/books?edit=b12 (desde el titulo de un lector): el editor de ese libro abierto.
  $effect(() => {
    const want = route.query.get("edit");
    if (!want || !books) return;
    const b = books.find((x) => x.id === want || x.id === `b${want}`);
    setQuery({});
    if (b && (session.user?.admin || session.user?.name === b.uploader)) {
      editing = b;
      editTitle = b.title;
      editAuthor = b.author ?? "";
    }
  });
  api
    .history(200)
    .then((h) => (progress = Object.fromEntries(h.filter((e) => e.work.kind === "book").map((e) => [e.work.id, e.progress.fraction]))))
    .catch(() => {});

  const norm = (s: string) => s.normalize("NFD").replace(/\p{Diacritic}/gu, "").toLowerCase();
  const shown = $derived.by(() => {
    if (!books) return null;
    const n = norm(q.trim());
    return books.filter((b) => {
      const f = filter === "all" || (filter === "other" ? !["epub", "pdf", "cbz"].includes(b.format) : b.format === filter);
      return f && (!n || norm(b.title).includes(n) || norm(b.author ?? "").includes(n));
    });
  });

  function upload(files: File[]) {
    const ok = files.filter((f) => EXT.test(f.name));
    if (ok.length < files.length) toast(t("books.unsupported", { n: files.length - ok.length }), true);
    // Una subida por archivo (cada una con su barra), de a dos a la vez.
    const queue = [...ok];
    const work = async () => {
      for (let f = queue.shift(); f; f = queue.shift()) {
        const id = nextId++;
        const u = uploadBooks([f], (frac) => {
          const it = uploads.find((x) => x.id === id);
          if (it) it.frac = frac;
        });
        uploads.push({ id, name: f.name, size: f.size, frac: 0, error: "", abort: u.abort });
        try {
          const got = await u.done;
          books = [...got, ...(books ?? [])];
          uploads = uploads.filter((x) => x.id !== id);
        } catch (e) {
          if ((e as Error).name === "AbortError") uploads = uploads.filter((x) => x.id !== id);
          else {
            const it = uploads.find((x) => x.id === id);
            if (it) it.error = errText(e);
          }
        }
      }
    };
    work();
    if (queue.length) work();
  }

  // Soltar archivos en la vista. dragenter/leave se disparan en cada hijo: se cuenta.
  let depth = 0;
  function dragEnter(e: DragEvent) {
    if (!e.dataTransfer?.types.includes("Files")) return;
    e.preventDefault();
    depth++;
    over = true;
  }
  function dragLeave() {
    depth = Math.max(0, depth - 1);
    if (!depth) over = false;
  }
  function drop(e: DragEvent) {
    e.preventDefault();
    depth = 0;
    over = false;
    if (e.dataTransfer?.files.length) upload([...e.dataTransfer.files]);
  }

  function bookMenu(b: Book, x: number, y: number) {
    const mine = session.user?.admin || session.user?.name === b.uploader;
    if (!mine) return;
    const items: PopItem[] = [
      {
        label: t("books.edit"),
        icon: Pencil,
        run: () => {
          editing = b;
          editTitle = b.title;
          editAuthor = b.author ?? "";
        },
      },
    ];
    items.push({
        label: t("common.delete"),
        icon: Trash2,
        run: async () => {
          if (!(await ask({ title: t("books.deleteTitle", { title: b.title }), body: t("books.deleteBody"), confirm: t("common.delete"), danger: true }))) return;
          await attempt(async () => {
            await api.deleteBook(b.id);
            books = (books ?? []).filter((x) => x.id !== b.id);
          }, t("books.deleted"));
        },
      });
    menu = { x, y, items };
  }

  async function saveEdit() {
    const b = editing;
    if (!b) return;
    const ok = await attempt(async () => {
      const n = await api.editBook(b.id, { title: editTitle.trim() || b.title, author: editAuthor.trim() });
      books = (books ?? []).map((x) => (x.id === b.id ? n : x));
    });
    if (ok) editing = null;
  }
</script>

<div class="view books" ondragenter={dragEnter} ondragleave={dragLeave} ondragover={(e) => e.preventDefault()} ondrop={drop} role="region" aria-label={t("nav.books")}>
  <div class="page-head">
    <h1>{t("nav.books")}</h1>
    {#if books}<span class="sub">{t("books.count", { n: books.length })}</span>{/if}
    <span class="grow"></span>
    <button class="btn primary" onclick={() => picker?.click()}><BookUp size={16} />{t("books.upload")}</button>
    <input bind:this={picker} type="file" accept={ACCEPT} multiple hidden onchange={(e) => (upload([...(e.currentTarget.files ?? [])]), (e.currentTarget.value = ""))} />
  </div>

  {#if books?.length}
    <div class="bar">
      <div class="chips-scroll">
        {#each [["all", t("books.all")], ["epub", "EPUB"], ["pdf", "PDF"], ["cbz", "CBZ"], ["other", t("books.other")]] as [id, label] (id)}
          <button class="pick" class:on={filter === id} onclick={() => (filter = id as typeof filter)}>{label}</button>
        {/each}
      </div>
      <div class="inbox find">
        <Search size={15} />
        <input bind:value={q} placeholder={t("books.filter")} />
        {#if q}<button class="icon-btn" onclick={() => (q = "")} aria-label={t("common.clear")}><X size={14} /></button>{/if}
      </div>
    </div>
  {/if}

  {#if uploads.length}
    <div class="uploads">
      {#each uploads as u (u.id)}
        <div class="up card" class:bad={!!u.error} in:rise>
          <span class="up-ico">{#if u.error}<X size={18} />{:else}<LoaderCircle size={18} class="spin" />{/if}</span>
          <span class="up-txt">
            <span class="ellipsis up-name">{u.name}</span>
            <span class="faint up-sub">{u.error || `${bytes(u.size)} · ${Math.round(u.frac * 100)} %`}</span>
            {#if !u.error}<span class="progress"><span class="progress-fill" style="width: {u.frac * 100}%"></span></span>{/if}
          </span>
          <button class="icon-btn" onclick={() => (u.error ? (uploads = uploads.filter((x) => x.id !== u.id)) : u.abort())} aria-label={t("common.cancel")}><X size={15} /></button>
        </div>
      {/each}
    </div>
  {/if}

  {#if books && !books.length && !uploads.length}
    <button class="drop-empty" onclick={() => picker?.click()}>
      <FileUp size={36} />
      <h3>{t("books.empty")}</h3>
      <p class="muted">{t("books.emptyHint")}</p>
      <span class="faint formats">EPUB · PDF · CBZ · MOBI · AZW3 · FB2 · TXT</span>
    </button>
  {:else if shown && !shown.length && books?.length}
    <div class="empty"><h3>{t("books.noMatch")}</h3></div>
  {:else}
    <CoverGrid items={shown} skeletons={12}>
      {#snippet card(b: Book, i: number)}
        <CoverCard work={b} {i} sub={b.author ?? ""} progress={progress[b.id] ?? null} onmenu={(x, y) => bookMenu(b, x, y)} />
      {/snippet}
    </CoverGrid>
  {/if}

  {#if over}
    <div class="dropzone" transition:fadeQuick>
      <div class="dz-inner"><FileUp size={40} /><span>{t("books.dropHere")}</span></div>
    </div>
  {/if}
</div>

{#if menu}<PopMenu x={menu.x} y={menu.y} items={menu.items} onclose={() => (menu = null)} />{/if}

{#if editing}
  <Drawer title={t("books.edit")} onclose={() => (editing = null)} onsave={saveEdit}>
    <label class="field">{t("books.titleField")}<input bind:value={editTitle} /></label>
    <label class="field">{t("books.author")}<input bind:value={editAuthor} /></label>
    <div class="faint meta">
      {editing.format.toUpperCase()} · {bytes(editing.size)} · {t("books.uploadedBy", { name: editing.uploader })}
    </div>
    {#snippet footer()}
      <button class="btn" onclick={() => (editing = null)}>{t("common.cancel")}</button>
      <button class="btn primary" onclick={saveEdit}>{t("common.save")}</button>
    {/snippet}
  </Drawer>
{/if}

<style>
  .books {
    position: relative;
    min-height: 100%;
  }
  .bar {
    display: flex;
    align-items: center;
    gap: 10px;
    margin-bottom: 18px;
    flex-wrap: wrap;
  }
  .bar .chips-scroll {
    flex: 1 1 auto;
    min-width: 0;
  }
  .find {
    width: min(280px, 100%);
    height: 36px;
  }
  .find .icon-btn {
    width: 24px;
    height: 24px;
  }
  .uploads {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(260px, 1fr));
    gap: 10px;
    margin-bottom: 20px;
  }
  .up {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 10px 10px 10px 14px;
  }
  .up.bad {
    border-color: var(--danger);
  }
  .up.bad .up-ico {
    color: var(--danger);
  }
  .up-ico {
    color: var(--accent);
    display: grid;
  }
  .up-txt {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .up-name {
    font-weight: 600;
  }
  .up-sub {
    font-size: 11.5px;
  }
  .drop-empty {
    width: 100%;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 10px;
    padding: 70px 20px;
    border: 2px dashed var(--line-strong);
    border-radius: calc(var(--radius) + 4px);
    color: var(--text-2);
    text-align: center;
    transition:
      border-color 0.2s,
      background 0.2s;
  }
  .drop-empty:hover {
    border-color: var(--accent);
    background: color-mix(in srgb, var(--accent) 6%, transparent);
  }
  .drop-empty :global(svg) {
    color: var(--accent);
  }
  .drop-empty h3 {
    margin: 0;
    color: var(--text);
  }
  .drop-empty p {
    margin: 0;
  }
  .formats {
    font-size: 11.5px;
    letter-spacing: 0.05em;
  }
  .dropzone {
    position: fixed;
    inset: 0;
    z-index: 70;
    display: grid;
    place-items: center;
    padding: 24px;
    background: color-mix(in srgb, var(--bg) 70%, transparent);
    backdrop-filter: blur(6px);
    pointer-events: none;
  }
  .dz-inner {
    width: 100%;
    height: 100%;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 14px;
    border: 3px dashed var(--accent);
    border-radius: 24px;
    color: var(--accent);
    font-size: 20px;
    font-weight: 700;
    animation: dz 1.2s ease-in-out infinite alternate;
  }
  @keyframes dz {
    to {
      background: color-mix(in srgb, var(--accent) 8%, transparent);
    }
  }
  .meta {
    font-size: 12px;
  }
</style>
