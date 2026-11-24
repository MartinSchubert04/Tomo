<script lang="ts">
  // Detalle de un libro de un catalogo gratis: portada, titulo, autor, idioma, año, formatos y
  // sinopsis (la pide al server si la lista no la traia). "Agregar a mis libros" lo baja al server
  // (queda para todos) y el boton pasa a "Leer".
  import { BookOpen, Check, ExternalLink, LoaderCircle, Plus } from "@lucide/svelte";
  import Drawer from "./Drawer.svelte";
  import Img from "./Img.svelte";
  import { api, errText } from "../lib/api";
  import { langName } from "../lib/catalogs";
  import { t } from "../lib/i18n.svelte";
  import { go } from "../lib/router.svelte";
  import { toast } from "../lib/toast.svelte";
  import type { Book, Catalog, CatalogEntry } from "../lib/types";

  let {
    catalog,
    entry: given,
    onclose,
    onimported,
  }: {
    catalog: Catalog;
    entry: CatalogEntry;
    onclose: () => void;
    onimported?: (entry: CatalogEntry, book: Book) => void;
  } = $props();

  // svelte-ignore state_referenced_locally
  let entry = $state<CatalogEntry>({ ...given });
  let busy = $state(false);
  let more = $state(false);

  // Elejandria e Internet Archive tienen ficha (sinopsis, titulo con acentos); OPDS ya trae todo.
  $effect(() => {
    if (catalog.kind === "opds") return;
    let alive = true;
    api
      .catalogEntry(catalog.id, given.id)
      .then((d) => alive && (entry = { ...entry, ...d, cover: entry.cover ?? d.cover }))
      .catch(() => {});
    return () => (alive = false);
  });

  async function add() {
    if (busy) return;
    busy = true;
    try {
      const b = await api.catalogImport(catalog.id, entry);
      entry = { ...entry, imported: b.id };
      onimported?.(entry, b);
      toast(t("cat.added", { title: b.title }));
    } catch (e) {
      toast(errText(e), true);
    } finally {
      busy = false;
    }
  }

  function read() {
    if (!entry.imported) return;
    onclose();
    go(`/book/${entry.imported}`);
  }
</script>

<Drawer title={t("cat.details")} {onclose}>
  <div class="head">
    <div class="cover"><Img src={entry.cover} alt="" fallback={entry.title} fallbackSub={entry.author ?? ""} /></div>
    <div class="info">
      <h2 class="title">{entry.title}</h2>
      {#if entry.author}<div class="author">{entry.author}</div>{/if}
      <div class="chips">
        <span class="chip">{catalog.name}</span>
        {#if entry.language}<span class="chip">{langName(entry.language)}</span>{/if}
        {#if entry.year}<span class="chip">{entry.year}</span>{/if}
        {#each entry.formats as f (f)}<span class="chip mono">{f.toUpperCase()}</span>{/each}
      </div>
    </div>
  </div>

  <div class="actions">
    {#if entry.imported}
      <button class="btn primary" onclick={read}><BookOpen size={16} />{t("cat.read")}</button>
      <span class="ok faint"><Check size={14} />{t("cat.inBooks")}</span>
    {:else if entry.formats.length}
      <button class="btn primary" disabled={busy} onclick={add}>
        {#if busy}<LoaderCircle size={16} class="spin" />{t("cat.adding")}{:else}<Plus size={16} />{t("cat.add")}{/if}
      </button>
    {:else}
      <span class="faint">{t("cat.noFormats")}</span>
    {/if}
    {#if entry.url}
      <a class="btn ghost" href={entry.url} target="_blank" rel="noopener noreferrer"><ExternalLink size={15} />{t("cat.openSite")}</a>
    {/if}
  </div>

  {#if entry.summary}
    <p class="summary" class:clamp={!more}>{entry.summary}</p>
    {#if entry.summary.length > 420}
      <button class="link-btn" onclick={() => (more = !more)}>{more ? t("common.less") : t("common.more")}</button>
    {/if}
  {/if}
</Drawer>

<style>
  .head {
    display: flex;
    gap: 16px;
    align-items: flex-start;
  }
  .cover {
    position: relative;
    flex: none;
    width: 118px;
    aspect-ratio: 2 / 3;
    border-radius: var(--radius-sm);
    overflow: hidden;
    background: var(--panel-2);
    box-shadow: var(--card-shadow);
  }
  .info {
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .title {
    margin: 0;
    font-size: 19px;
    line-height: 1.25;
    font-weight: 700;
  }
  .author {
    color: var(--text-2);
  }
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 5px;
    margin-top: 4px;
  }
  .actions {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 10px;
    margin: 18px 0 6px;
  }
  .ok {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    color: var(--ok);
  }
  .summary {
    margin: 12px 0 4px;
    font-size: 14px;
    line-height: 1.6;
    color: var(--text-2);
    white-space: pre-line;
  }
  .clamp {
    display: -webkit-box;
    -webkit-line-clamp: 8;
    line-clamp: 8;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }
  .link-btn {
    color: var(--accent);
    font-weight: 650;
  }
</style>
