<script lang="ts">
  // Admin → Catalogos: los catalogos de libros gratis (prender/apagar los de fabrica, agregar o
  // quitar OPDS propios) y la carpeta de entrada del server (donde esta, que espera, que fallo).
  import { FolderInput, LoaderCircle, Plus, RefreshCw, Trash2, TriangleAlert } from "@lucide/svelte";
  import Switch from "./Switch.svelte";
  import { api, errText } from "../lib/api";
  import { loadCatalogs } from "../lib/catalogs";
  import { ask } from "../lib/confirm.svelte";
  import { t } from "../lib/i18n.svelte";
  import { rise } from "../lib/motion";
  import { attempt, toast } from "../lib/toast.svelte";
  import type { Catalog, InboxStatus } from "../lib/types";

  let cats = $state<Catalog[] | null>(null);
  let url = $state("");
  let adding = $state(false);
  let inbox = $state<InboxStatus | null>(null);
  let scanning = $state(false);

  api.adminCatalogs().then((c) => (cats = c)).catch((e) => ((cats = []), toast(errText(e), true)));
  api.inbox().then((i) => (inbox = i)).catch(() => {});

  async function toggle(c: Catalog, on: boolean) {
    const ok = await attempt(() => api.editCatalog(c.id, { enabled: on }));
    if (ok) {
      cats = (cats ?? []).map((x) => (x.id === c.id ? { ...x, enabled: on } : x));
      loadCatalogs(true).catch(() => {});
    }
  }

  async function add(e: SubmitEvent) {
    e.preventDefault();
    const u = url.trim();
    if (!u || adding) return;
    adding = true;
    try {
      const c = await api.addCatalog(u);
      cats = [...(cats ?? []), c];
      url = "";
      toast(t("admin.catalogAdded"));
      loadCatalogs(true).catch(() => {});
    } catch (err) {
      toast(errText(err), true);
    } finally {
      adding = false;
    }
  }

  async function remove(c: Catalog) {
    if (!(await ask({ title: t("admin.removeCatalogTitle", { name: c.name }), body: t("admin.removeCatalogBody"), confirm: t("common.remove"), danger: true }))) return;
    const ok = await attempt(() => api.removeCatalog(c.id));
    if (ok) {
      cats = (cats ?? []).filter((x) => x.id !== c.id);
      loadCatalogs(true).catch(() => {});
    }
  }

  async function scan() {
    scanning = true;
    try {
      inbox = await api.scanInbox();
      toast(t("admin.scanned"));
    } catch (e) {
      toast(errText(e), true);
    } finally {
      scanning = false;
    }
  }
</script>

<div class="hint-top faint">{t("admin.catalogsHint")}</div>
<div class="list">
  {#if cats}
    {#each cats as c (c.id)}
      <div class="cat card" in:rise>
        <div class="info">
          <span class="name">{c.name}</span>
          <span class="tags">
            {#if c.preset}<span class="chip sm-chip">{t("admin.builtin")}</span>{/if}
            {#if c.spanish}<span class="chip ok sm-chip">{t("admin.spanish")}</span>{/if}
            <span class="chip sm-chip mono">{c.kind.toUpperCase()}</span>
          </span>
          {#if c.url}<span class="mono faint ellipsis url">{c.url}</span>{/if}
        </div>
        <Switch checked={c.enabled} onchange={(v) => toggle(c, v)} />
        {#if !c.preset}
          <button class="icon-btn danger-ico" title={t("common.remove")} aria-label={t("common.remove")} onclick={() => remove(c)}><Trash2 size={15} /></button>
        {/if}
      </div>
    {/each}
    <form class="add" onsubmit={add}>
      <input bind:value={url} placeholder={t("admin.catalogUrl")} inputmode="url" />
      <button class="btn primary" disabled={!url.trim() || adding}>
        {#if adding}<LoaderCircle size={14} class="spin" />{:else}<Plus size={14} />{/if}{t("admin.addCatalog")}
      </button>
    </form>
  {:else}
    {#each Array(3) as _, i (i)}<div class="skel cat-skel"></div>{/each}
  {/if}
</div>

<h2 class="section-title inbox-title"><FolderInput size={14} />{t("admin.inbox")}</h2>
<div class="hint-top faint">{t("admin.inboxHint")}</div>
<div class="card inbox-card">
  {#if inbox}
    <div class="paths">
      {#if inbox.hostPath}
        <span class="faint">{t("admin.inboxPath")}</span><code class="ellipsis">{inbox.hostPath}</code>
      {/if}
      <span class="faint">{t("admin.inboxContainer")}</span><code class="ellipsis">{inbox.path}</code>
    </div>
    <div class="state">
      <span>{inbox.pending.length ? t("admin.inboxPending", { n: inbox.pending.length }) : t("admin.inboxEmpty")}</span>
      <span class="grow"></span>
      <button class="btn sm" onclick={scan} disabled={scanning}>
        {#if scanning}<LoaderCircle size={13} class="spin" />{:else}<RefreshCw size={13} />{/if}{t("admin.scanNow")}
      </button>
    </div>
    {#if inbox.failed.length}
      <div class="failed">
        <span class="warn-title"><TriangleAlert size={14} />{t("admin.inboxFailed")}</span>
        {#each inbox.failed as f (f.name)}
          <div class="fail"><span class="mono ellipsis">{f.name}</span><span class="faint ellipsis">{f.error}</span></div>
        {/each}
      </div>
    {/if}
  {:else}
    <span class="skel line" style="width: 60%"></span>
  {/if}
</div>

<style>
  .hint-top {
    margin: -4px 0 14px;
    font-size: 12.5px;
    max-width: 760px;
  }
  .list {
    display: flex;
    flex-direction: column;
    gap: 8px;
    max-width: 760px;
  }
  .cat {
    display: flex;
    align-items: center;
    gap: 14px;
    padding: 12px 14px;
  }
  .info {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .name {
    font-weight: 650;
  }
  .tags {
    display: flex;
    flex-wrap: wrap;
    gap: 5px;
  }
  .url {
    font-size: 11.5px;
  }
  .sm-chip {
    height: 17px;
    font-size: 10px;
    padding: 0 6px;
  }
  .cat-skel {
    height: 64px;
    border-radius: var(--radius);
  }
  .add {
    display: flex;
    gap: 8px;
    margin-top: 4px;
  }
  .add input {
    flex: 1;
    min-width: 0;
  }
  .danger-ico:hover {
    color: var(--danger);
  }
  .inbox-title {
    display: flex;
    align-items: center;
    gap: 7px;
    margin: 30px 0 10px;
  }
  .inbox-card {
    max-width: 760px;
    padding: 14px;
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  .paths {
    display: grid;
    grid-template-columns: auto minmax(0, 1fr);
    gap: 6px 12px;
    align-items: center;
    font-size: 12.5px;
  }
  .state {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .failed {
    display: flex;
    flex-direction: column;
    gap: 6px;
    border-top: var(--border) solid var(--line);
    padding-top: 10px;
  }
  .warn-title {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    color: var(--warn);
    font-weight: 600;
    font-size: 12.5px;
  }
  .fail {
    display: grid;
    grid-template-columns: minmax(0, 1fr) minmax(0, 2fr);
    gap: 10px;
    font-size: 12px;
  }
  @media (max-width: 759px) {
    .fail {
      grid-template-columns: 1fr;
      gap: 2px;
    }
  }
</style>
