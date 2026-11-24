<script lang="ts">
  // Libros, en dos pestañas (la elegida queda en la URL: #/books?tab=explore):
  // - "Mis libros": todo lo subido, bajado de un catalogo o importado de la carpeta de entrada
  //   (compartido entre usuarios). Agrupado por saga ("La rueda del tiempo · 3 libros", en orden)
  //   o todo junto; filtro por formato, idioma y texto; orden. Subir: el boton o soltar archivos en
  //   cualquier parte (aparece "Solta para subir"); cada subida es una tarjeta con su barra.
  // - "Libros gratis": los catalogos gratis y legales (components/CatalogBrowser.svelte).
  import { BookUp, FileUp, LoaderCircle, Pencil, Search, Trash2, X } from "@lucide/svelte";
  import CatalogBrowser from "../components/CatalogBrowser.svelte";
  import CoverCard from "../components/CoverCard.svelte";
  import CoverGrid from "../components/CoverGrid.svelte";
  import Drawer from "../components/Drawer.svelte";
  import PopMenu, { type PopItem } from "../components/PopMenu.svelte";
  import Seg from "../components/Seg.svelte";
  import Select from "../components/Select.svelte";
  import { route, setQuery } from "../lib/router.svelte";
  import { api, errText, uploadBooks } from "../lib/api";
  import { langName } from "../lib/catalogs";
  import { ask } from "../lib/confirm.svelte";
  import { bytes } from "../lib/format";
  import { t } from "../lib/i18n.svelte";
  import { fadeQuick, rise } from "../lib/motion";
  import { session } from "../lib/session.svelte";
  import { read, write } from "../lib/store";
  import { attempt, toast } from "../lib/toast.svelte";
  import type { Book } from "../lib/types";

  const ACCEPT = ".epub,.pdf,.cbz,.zip,.mobi,.azw3,.azw,.fb2,.txt";
  const EXT = /\.(epub|pdf|cbz|zip|mobi|azw3?|fb2|txt)$/i;
  const GROUP_KEY = "library.booksGroup";
  const SORT_KEY = "library.booksSort";

  type Tab = "mine" | "explore";
  type Sort = "recent" | "title" | "author";
  type Up = { id: number; name: string; size: number; frac: number; error: string; abort: () => void };

  const tab = $derived<Tab>(route.query.get("tab") === "explore" ? "explore" : "mine");

  let books = $state<Book[] | null>(null);
  let progress = $state<Record<string, number>>({});
  let uploads = $state<Up[]>([]);
  let filter = $state<"all" | "epub" | "pdf" | "cbz" | "other">("all");
  let lang = $state("");
  let q = $state("");
  let grouped = $state(read(GROUP_KEY) !== "0");
  let sort = $state<Sort>((read(SORT_KEY) as Sort) || "recent");
  let over = $state(false);
  let picker = $state<HTMLInputElement>();
  let editing = $state<Book | null>(null);
  let form = $state({ title: "", author: "", series: "", seriesIndex: "", language: "", year: "", description: "" });
  let menu = $state<{ x: number; y: number; items: PopItem[] } | null>(null);
  let nextId = 1;

  function loadBooks() {
    api.books().then((b) => (books = b)).catch((e) => ((books = []), toast(errText(e), true)));
  }
  loadBooks();

  function startEdit(b: Book) {
    editing = b;
    form = {
      title: b.title,
      author: b.author ?? "",
      series: b.series ?? "",
      seriesIndex: b.seriesIndex != null ? String(b.seriesIndex) : "",
      language: b.language ?? "",
      year: b.year != null ? String(b.year) : "",
      description: b.description ?? "",
    };
  }
  const canEdit = (b: Book) => !!session.user && (session.user.admin || session.user.name === b.uploader);

  // #/books?edit=b12 (desde el titulo de un lector): el editor de ese libro abierto.
  $effect(() => {
    const want = route.query.get("edit");
    if (!want || !books) return;
    const b = books.find((x) => x.id === want || x.id === `b${want}`);
    setQuery({});
    if (b && canEdit(b)) startEdit(b);
  });
  api
    .history(200)
    .then((h) => (progress = Object.fromEntries(h.filter((e) => e.work.kind === "book").map((e) => [e.work.id, e.progress.fraction]))))
    .catch(() => {});

  const norm = (s: string) => s.normalize("NFD").replace(/\p{Diacritic}/gu, "").toLowerCase();
  const langs = $derived([...new Set((books ?? []).map((b) => b.language).filter((l): l is string => !!l))].sort());
  const shown = $derived.by(() => {
    if (!books) return null;
    const n = norm(q.trim());
    const out = books.filter((b) => {
      const f = filter === "all" || (filter === "other" ? !["epub", "pdf", "cbz"].includes(b.format) : b.format === filter);
      const l = !lang || b.language === lang;
      return f && l && (!n || norm(b.title).includes(n) || norm(b.author ?? "").includes(n) || norm(b.series ?? "").includes(n));
    });
    const by = (k: (b: Book) => string) => (a: Book, b: Book) => k(a).localeCompare(k(b), undefined, { sensitivity: "base", numeric: true });
    if (sort === "title") out.sort(by((b) => b.title));
    else if (sort === "author") out.sort(by((b) => b.author ?? "￿"));
    return out;
  });
  // Sagas: las que tienen al menos un libro a la vista, en orden de numero; el resto, "Sueltos".
  const series = $derived.by(() => {
    if (!shown || !grouped) return null;
    const m = new Map<string, Book[]>();
    for (const b of shown) if (b.series) m.set(b.series, [...(m.get(b.series) ?? []), b]);
    const list = [...m.entries()].map(([name, l]) => ({ name, books: l.sort((a, b) => (a.seriesIndex ?? 1e9) - (b.seriesIndex ?? 1e9)) }));
    list.sort((a, b) => a.name.localeCompare(b.name, undefined, { sensitivity: "base" }));
    return { list, rest: shown.filter((b) => !b.series) };
  });

  function sub(b: Book) {
    if (b.series && b.seriesIndex != null && !grouped) return t("books.inSeries", { series: b.series, n: b.seriesIndex });
    return b.author ?? "";
  }

  function upload(files: File[]) {
    const ok = files.filter((f) => EXT.test(f.name));
    if (ok.length < files.length) toast(t("books.unsupported", { n: files.length - ok.length }), true);
    if (ok.length && tab !== "mine") setQuery({});
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
    if (!canEdit(b)) return;
    const items: PopItem[] = [{ label: t("books.edit"), icon: Pencil, run: () => startEdit(b) }];
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
    const idx = form.seriesIndex.trim() ? Number(form.seriesIndex.replace(",", ".")) : undefined;
    const year = form.year.trim() ? Number(form.year) : undefined;
    const ok = await attempt(async () => {
      const n = await api.editBook(b.id, {
        title: form.title.trim() || b.title,
        author: form.author.trim(),
        series: form.series.trim(),
        seriesIndex: idx !== undefined && Number.isFinite(idx) ? idx : undefined,
        language: form.language.trim(),
        year: year !== undefined && Number.isFinite(year) ? year : undefined,
        description: form.description.trim(),
      });
      books = (books ?? []).map((x) => (x.id === b.id ? n : x));
    });
    if (ok) editing = null;
  }

  function setGrouped(v: boolean) {
    grouped = v;
    write(GROUP_KEY, v ? "1" : "0");
  }
  function setSort(v: Sort) {
    sort = v;
    write(SORT_KEY, v);
  }
  const origin = (b: Book) => (b.origin === "inbox" ? t("books.fromInbox") : b.origin ? t("books.fromCatalog", { name: b.origin }) : t("books.uploadedBy", { name: b.uploader }));
</script>

<div class="view books" ondragenter={dragEnter} ondragleave={dragLeave} ondragover={(e) => e.preventDefault()} ondrop={drop} role="region" aria-label={t("nav.books")}>
  <div class="page-head">
    <h1>{t("nav.books")}</h1>
    {#if books && tab === "mine"}<span class="sub">{t("books.count", { n: books.length })}</span>{/if}
    <span class="grow"></span>
    <button class="btn primary" onclick={() => picker?.click()}><BookUp size={16} /><span class="hide-sm">{t("books.upload")}</span></button>
    <input bind:this={picker} type="file" accept={ACCEPT} multiple hidden onchange={(e) => (upload([...(e.currentTarget.files ?? [])]), (e.currentTarget.value = ""))} />
  </div>

  <div class="tabs">
    <Seg
      value={tab}
      options={[
        { value: "mine" as Tab, label: t("books.tabMine") },
        { value: "explore" as Tab, label: t("books.tabExplore") },
      ]}
      onchange={(v) => setQuery(v === "explore" ? { tab: "explore" } : {})}
    />
  </div>

  {#if tab === "explore"}
    {#key "explore"}
      <div in:rise><CatalogBrowser onimported={(b) => (books = [b, ...(books ?? []).filter((x) => x.id !== b.id)])} /></div>
    {/key}
  {:else}
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
        <div class="tools">
          {#if langs.length > 1}
            <div class="sel">
              <Select value={lang} options={[{ value: "", label: t("books.anyLang") }, ...langs.map((l) => ({ value: l, label: langName(l) }))]} onchange={(v) => (lang = v)} />
            </div>
          {/if}
          <div class="sel">
            <Select
              value={sort}
              options={[
                { value: "recent" as Sort, label: t("books.sortRecent") },
                { value: "title" as Sort, label: t("books.sortTitle") },
                { value: "author" as Sort, label: t("books.sortAuthor") },
              ]}
              onchange={setSort}
            />
          </div>
          <Seg
            value={grouped}
            options={[
              { value: true, label: t("books.bySeries") },
              { value: false, label: t("books.flat") },
            ]}
            onchange={setGrouped}
          />
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
      <div class="first">
        <button class="drop-empty" onclick={() => picker?.click()}>
          <FileUp size={36} />
          <h3>{t("books.empty")}</h3>
          <p class="muted">{t("books.emptyHint")}</p>
          <span class="faint formats">EPUB · PDF · CBZ · MOBI · AZW3 · FB2 · TXT</span>
        </button>
        <button class="btn" onclick={() => setQuery({ tab: "explore" })}>{t("books.tabExplore")} →</button>
      </div>
    {:else if shown && !shown.length && books?.length}
      <div class="empty"><h3>{t("books.noMatch")}</h3></div>
    {:else if series && (series.list.length || !books)}
      {#each series.list as s (s.name)}
        <section class="series" in:rise>
          <h2 class="series-title"><span class="ellipsis">{s.name}</span><span class="faint count">{t("books.count", { n: s.books.length })}</span></h2>
          <CoverGrid items={s.books} skeletons={0}>
            {#snippet card(b: Book, i: number)}
              <CoverCard work={b} {i} sub={b.author ?? ""} badge={b.seriesIndex != null ? `#${b.seriesIndex}` : ""} progress={progress[b.id] ?? null} onmenu={(x, y) => bookMenu(b, x, y)} />
            {/snippet}
          </CoverGrid>
        </section>
      {/each}
      {#if series.rest.length}
        <section class="series">
          <h2 class="series-title"><span>{t("books.standalone")}</span><span class="faint count">{t("books.count", { n: series.rest.length })}</span></h2>
          <CoverGrid items={series.rest} skeletons={0}>
            {#snippet card(b: Book, i: number)}
              <CoverCard work={b} {i} sub={sub(b)} progress={progress[b.id] ?? null} onmenu={(x, y) => bookMenu(b, x, y)} />
            {/snippet}
          </CoverGrid>
        </section>
      {/if}
    {:else}
      <CoverGrid items={shown} skeletons={12}>
        {#snippet card(b: Book, i: number)}
          <CoverCard work={b} {i} sub={sub(b)} progress={progress[b.id] ?? null} onmenu={(x, y) => bookMenu(b, x, y)} />
        {/snippet}
      </CoverGrid>
    {/if}
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
    <label class="field">{t("books.titleField")}<input bind:value={form.title} /></label>
    <label class="field">{t("books.author")}<input bind:value={form.author} /></label>
    <div class="row2">
      <label class="field grow">{t("books.series")}<input bind:value={form.series} placeholder="La rueda del tiempo" /></label>
      <label class="field num">{t("books.seriesIndex")}<input bind:value={form.seriesIndex} inputmode="decimal" placeholder="1" /></label>
    </div>
    <span class="hint faint">{t("books.seriesHint")}</span>
    <div class="row2">
      <label class="field grow">{t("books.language")}<input bind:value={form.language} placeholder="es" /></label>
      <label class="field num">{t("books.year")}<input bind:value={form.year} inputmode="numeric" placeholder="1990" /></label>
    </div>
    <label class="field">{t("books.description")}<textarea bind:value={form.description} rows="5"></textarea></label>
    <div class="faint meta">
      {editing.format.toUpperCase()} · {bytes(editing.size)} · {origin(editing)}
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
  .tabs {
    display: flex;
    margin: -4px 0 18px;
  }
  .bar {
    display: flex;
    align-items: center;
    gap: 10px 14px;
    margin-bottom: 18px;
    flex-wrap: wrap;
  }
  .bar .chips-scroll {
    flex: 0 1 auto;
    min-width: 0;
  }
  .tools {
    display: flex;
    align-items: center;
    gap: 10px;
    flex-wrap: wrap;
    justify-content: flex-end;
    margin-left: auto;
  }
  .sel {
    min-width: 140px;
  }
  .find {
    width: min(240px, 100%);
    height: 36px;
  }
  .find .icon-btn {
    width: 24px;
    height: 24px;
  }
  .series {
    margin-bottom: 30px;
  }
  .series-title {
    display: flex;
    align-items: baseline;
    gap: 10px;
    margin: 0 0 12px;
    font-size: 17px;
    font-weight: 680;
    min-width: 0;
  }
  .count {
    flex: none;
    font-size: 12.5px;
    font-weight: 500;
    font-family: var(--font);
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
  .first {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 14px;
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
  .row2 {
    display: flex;
    gap: 10px;
  }
  .row2 .grow {
    flex: 1;
    min-width: 0;
  }
  .row2 .num {
    width: 96px;
    flex: none;
  }
  .hint {
    display: block;
    font-size: 12px;
    margin: -6px 0 12px;
  }
  .meta {
    font-size: 12px;
  }
  @media (max-width: 759px) {
    .hide-sm {
      display: none;
    }
    .tools {
      width: 100%;
    }
    .find {
      width: 100%;
    }
  }
</style>
