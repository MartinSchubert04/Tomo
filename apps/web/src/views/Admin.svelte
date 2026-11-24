<script lang="ts">
  // Admin: usuarios (crear, cambiar contraseña, hacer admin, borrar), fuentes (las extensiones de
  // Suwayomi: filtro por idioma, busqueda, instalar / actualizar / desinstalar) y los repos de
  // extensiones. La pestaña queda en la URL (#/admin?tab=sources).
  import { Download, KeyRound, LoaderCircle, Plus, RefreshCw, Search, Shield, ShieldOff, Trash2, UserPlus, X } from "@lucide/svelte";
  import Drawer from "../components/Drawer.svelte";
  import Seg from "../components/Seg.svelte";
  import Switch from "../components/Switch.svelte";
  import { api, errText } from "../lib/api";
  import { ask, prompt } from "../lib/confirm.svelte";
  import { date, t } from "../lib/i18n.svelte";
  import { route, setQuery } from "../lib/router.svelte";
  import { session } from "../lib/session.svelte";
  import { loadSources } from "../lib/sources.svelte";
  import { attempt, toast } from "../lib/toast.svelte";
  import type { Extension, User } from "../lib/types";

  type Tab = "users" | "sources" | "repos";
  const tab = $derived((route.query.get("tab") as Tab) || "users");

  // ---------- usuarios ----------
  let users = $state<User[] | null>(null);
  let creating = $state(false);
  let nName = $state("");
  let nPass = $state("");
  let nAdmin = $state(false);

  function loadUsers() {
    api.users().then((u) => (users = u)).catch((e) => toast(errText(e), true));
  }
  loadUsers();

  async function createUser() {
    if (!nName.trim() || nPass.length < 6) return toast(t("admin.userInvalid"), true);
    const ok = await attempt(async () => {
      const u = await api.createUser(nName.trim(), nPass, nAdmin);
      users = [...(users ?? []), u];
    }, t("admin.userCreated"));
    if (ok) {
      creating = false;
      nName = nPass = "";
      nAdmin = false;
    }
  }
  async function resetPassword(u: User) {
    const p = await prompt({ title: t("admin.resetTitle", { name: u.name }), placeholder: t("settings.newPassword"), secret: true, confirm: t("common.save") });
    if (!p) return;
    if (p.length < 6) return toast(t("settings.passwordShort"), true);
    await attempt(() => api.updateUser(u.id, { password: p }), t("settings.passwordChanged"));
  }
  async function toggleAdmin(u: User) {
    await attempt(async () => {
      await api.updateUser(u.id, { admin: !u.admin });
      users = (users ?? []).map((x) => (x.id === u.id ? { ...x, admin: !u.admin } : x));
    });
  }
  async function removeUser(u: User) {
    if (!(await ask({ title: t("admin.deleteTitle", { name: u.name }), body: t("admin.deleteBody"), confirm: t("common.delete"), danger: true }))) return;
    await attempt(async () => {
      await api.deleteUser(u.id);
      users = (users ?? []).filter((x) => x.id !== u.id);
    });
  }

  // ---------- fuentes ----------
  let exts = $state<Extension[] | null>(null);
  let lang = $state<"es" | "all" | "installed">("es");
  let q = $state("");
  let busy = $state<Record<string, boolean>>({});
  let refreshing = $state(false);
  let extErr = $state("");

  async function loadExts() {
    extErr = "";
    try {
      exts = await api.extensions();
    } catch (e) {
      extErr = errText(e);
      exts = [];
    }
  }
  $effect(() => {
    if (tab === "sources" && !exts) loadExts();
  });

  const norm = (s: string) => s.normalize("NFD").replace(/\p{Diacritic}/gu, "").toLowerCase();
  const shownExts = $derived.by(() => {
    const n = norm(q.trim());
    return (exts ?? []).filter((e) => {
      if (lang === "installed" && !e.installed) return false;
      if (lang === "es" && !["es", "es-419", "all"].includes(e.lang)) return false;
      return !n || norm(e.name).includes(n) || e.pkg.includes(n);
    });
  });
  const installedN = $derived((exts ?? []).filter((e) => e.installed).length);
  const updatesN = $derived((exts ?? []).filter((e) => e.hasUpdate).length);

  async function act(e: Extension, action: "install" | "uninstall" | "update") {
    if (action === "uninstall" && !(await ask({ title: t("admin.uninstallTitle", { name: e.name }), confirm: t("admin.uninstall"), danger: true }))) return;
    busy = { ...busy, [e.pkg]: true };
    try {
      const n = await api.extensionAction(e.pkg, action);
      exts = (exts ?? []).map((x) => (x.pkg === e.pkg ? n : x));
      toast(t(`admin.done.${action}`, { name: e.name }));
      loadSources(true).catch(() => {});
    } catch (err) {
      toast(errText(err), true);
    } finally {
      const b = { ...busy };
      delete b[e.pkg];
      busy = b;
    }
  }
  async function refreshExts() {
    refreshing = true;
    try {
      exts = await api.refreshExtensions();
    } catch (e) {
      toast(errText(e), true);
    } finally {
      refreshing = false;
    }
  }

  // ---------- repos ----------
  let repos = $state<string[] | null>(null);
  let newRepo = $state("");
  $effect(() => {
    if (tab === "repos" && !repos) api.repos().then((r) => (repos = r)).catch((e) => toast(errText(e), true));
  });
  async function saveRepos(r: string[]) {
    await attempt(async () => {
      await api.setRepos(r);
      repos = r;
      exts = null;
    }, t("admin.reposSaved"));
  }

  // Mostrar de a 60 extensiones (la lista entera son ~1700).
  let limit = $state(60);
  $effect(() => {
    void lang;
    void q;
    limit = 60;
  });
</script>

<div class="view">
  <div class="page-head">
    <h1>{t("nav.admin")}</h1>
    <span class="grow"></span>
    <Seg
      value={tab}
      options={[
        { value: "users" as Tab, label: t("admin.users") },
        { value: "sources" as Tab, label: t("admin.sources") },
        { value: "repos" as Tab, label: t("admin.repos") },
      ]}
      onchange={(v) => setQuery({ tab: v })}
    />
  </div>

  {#if tab === "users"}
    <div class="toolbar">
      <span class="grow"></span>
      <button class="btn primary" onclick={() => (creating = true)}><UserPlus size={15} />{t("admin.newUser")}</button>
    </div>
    <div class="card table-wrap">
      <table class="list">
        <thead><tr><th>{t("admin.name")}</th><th>{t("admin.role")}</th><th class="hide-phone">{t("admin.created")}</th><th></th></tr></thead>
        <tbody>
          {#if users}
            {#each users as u (u.id)}
              <tr>
                <td><span class="uname"><span class="avatar">{u.name.slice(0, 1).toUpperCase()}</span>{u.name}</span></td>
                <td>{#if u.admin}<span class="chip accent"><Shield size={11} />{t("admin.admin")}</span>{:else}<span class="chip">{t("admin.reader")}</span>{/if}</td>
                <td class="hide-phone faint">{u.createdAt ? date(u.createdAt) : ""}</td>
                <td class="acts">
                  <button class="icon-btn" title={t("admin.resetPassword")} onclick={() => resetPassword(u)}><KeyRound size={15} /></button>
                  {#if u.id !== session.user?.id}
                    <button class="icon-btn" title={u.admin ? t("admin.removeAdmin") : t("admin.makeAdmin")} onclick={() => toggleAdmin(u)}>
                      {#if u.admin}<ShieldOff size={15} />{:else}<Shield size={15} />{/if}
                    </button>
                    <button class="icon-btn danger-ico" title={t("common.delete")} onclick={() => removeUser(u)}><Trash2 size={15} /></button>
                  {/if}
                </td>
              </tr>
            {/each}
          {:else}
            {#each Array(3) as _, i (i)}<tr><td colspan="4"><span class="skel line" style="width: {40 + i * 15}%"></span></td></tr>{/each}
          {/if}
        </tbody>
      </table>
    </div>
  {:else if tab === "sources"}
    <div class="toolbar">
      <Seg
        bind:value={lang}
        options={[
          { value: "es" as const, label: t("admin.lang.es") },
          { value: "all" as const, label: t("admin.lang.all") },
          { value: "installed" as const, label: t("admin.lang.installed", { n: installedN }) },
        ]}
      />
      <div class="inbox find">
        <Search size={15} />
        <input bind:value={q} placeholder={t("admin.searchExt")} />
        {#if q}<button class="icon-btn" onclick={() => (q = "")} aria-label={t("common.clear")}><X size={14} /></button>{/if}
      </div>
      <span class="grow"></span>
      {#if updatesN}<span class="chip warn">{t("admin.updates", { n: updatesN })}</span>{/if}
      <button class="btn" onclick={refreshExts} disabled={refreshing}><RefreshCw size={14} class={refreshing ? "spin" : ""} />{t("admin.refresh")}</button>
    </div>
    {#if extErr}
      <div class="empty"><h3>{t("admin.extFailed")}</h3><p class="muted">{extErr}</p><button class="btn" onclick={loadExts}>{t("common.retry")}</button></div>
    {:else if !exts}
      <div class="exts">{#each Array(8) as _, i (i)}<div class="skel ext-ph"></div>{/each}</div>
    {:else if !shownExts.length}
      <div class="empty"><h3>{t("admin.noExt")}</h3></div>
    {:else}
      <div class="exts">
        {#each shownExts.slice(0, limit) as e (e.pkg)}
          <div class="ext card" class:inst={e.installed}>
            <img src={e.icon} alt="" loading="lazy" />
            <div class="ext-txt">
              <span class="ext-name ellipsis">{e.name}</span>
              <span class="ext-meta">
                <span class="mono">{e.lang.toUpperCase()}</span> · v{e.version}
                {#if e.nsfw === "nsfw"}<span class="chip bad sm-chip">NSFW</span>{:else if e.nsfw === "mixed"}<span class="chip warn sm-chip">{t("admin.mixed")}</span>{/if}
                {#if e.obsolete}<span class="chip bad sm-chip">{t("admin.obsolete")}</span>{/if}
              </span>
            </div>
            {#if busy[e.pkg]}
              <span class="ext-busy"><LoaderCircle size={17} class="spin" /></span>
            {:else if e.installed}
              {#if e.hasUpdate}<button class="btn sm primary" onclick={() => act(e, "update")}><Download size={13} />{t("admin.update")}</button>{/if}
              <button class="btn sm" onclick={() => act(e, "uninstall")}>{t("admin.uninstall")}</button>
            {:else}
              <button class="btn sm primary" onclick={() => act(e, "install")}><Plus size={13} />{t("admin.install")}</button>
            {/if}
          </div>
        {/each}
      </div>
      {#if shownExts.length > limit}
        <div class="more"><button class="btn" onclick={() => (limit += 120)}>{t("admin.showMore", { n: shownExts.length - limit })}</button></div>
      {/if}
    {/if}
  {:else}
    <div class="hint-top faint">{t("admin.reposHint")}</div>
    <div class="repos">
      {#if repos}
        {#each repos as r, i (r)}
          <div class="repo card">
            <span class="mono ellipsis grow">{r}</span>
            <button class="icon-btn danger-ico" title={t("common.remove")} onclick={() => saveRepos(repos!.filter((_, j) => j !== i))}><Trash2 size={15} /></button>
          </div>
        {/each}
        <form
          class="repo-add"
          onsubmit={(e) => {
            e.preventDefault();
            const u = newRepo.trim();
            if (u && repos) saveRepos([...repos, u]).then(() => (newRepo = ""));
          }}
        >
          <input bind:value={newRepo} placeholder="https://…/index.min.json" />
          <button class="btn primary" disabled={!newRepo.trim()}><Plus size={14} />{t("admin.addRepo")}</button>
        </form>
      {:else}
        <span class="skel line" style="width: 60%"></span>
      {/if}
    </div>
  {/if}
</div>

{#if creating}
  <Drawer title={t("admin.newUser")} onclose={() => (creating = false)} onsave={createUser}>
    <label class="field">{t("admin.name")}<input bind:value={nName} autocomplete="off" autocapitalize="off" /></label>
    <label class="field">{t("login.password")}<input type="password" bind:value={nPass} autocomplete="new-password" /><span class="hint">{t("admin.passwordHint")}</span></label>
    <Switch bind:checked={nAdmin}>{t("admin.isAdmin")}</Switch>
    {#snippet footer()}
      <button class="btn" onclick={() => (creating = false)}>{t("common.cancel")}</button>
      <button class="btn primary" onclick={createUser}>{t("admin.create")}</button>
    {/snippet}
  </Drawer>
{/if}

<style>
  .toolbar {
    display: flex;
    align-items: center;
    gap: 10px;
    margin-bottom: 16px;
    flex-wrap: wrap;
  }
  .find {
    width: min(300px, 100%);
    height: 36px;
  }
  .find .icon-btn {
    width: 24px;
    height: 24px;
  }
  .table-wrap {
    overflow-x: auto;
  }
  .uname {
    display: inline-flex;
    align-items: center;
    gap: 9px;
    font-weight: 600;
  }
  .avatar {
    width: 26px;
    height: 26px;
    display: grid;
    place-items: center;
    border-radius: 50%;
    background: var(--primary-bg);
    color: var(--accent-ink);
    font-weight: 750;
    font-size: 12px;
  }
  .acts {
    text-align: right;
    white-space: nowrap;
  }
  .danger-ico:hover {
    color: var(--danger);
  }
  .exts {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(300px, 1fr));
    gap: 10px;
  }
  .ext-ph {
    height: 62px;
    border-radius: var(--radius);
  }
  .ext {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 10px 12px;
    min-width: 0;
    animation: rise 0.3s cubic-bezier(0.2, 0.8, 0.2, 1) backwards;
  }
  .ext.inst {
    border-color: color-mix(in srgb, var(--ok) 40%, var(--line));
  }
  .ext img {
    width: 38px;
    height: 38px;
    border-radius: 9px;
    flex: none;
    background: var(--panel-2);
  }
  .ext-txt {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 3px;
  }
  .ext-name {
    font-weight: 620;
  }
  .ext-meta {
    display: flex;
    align-items: center;
    gap: 5px;
    font-size: 11.5px;
    color: var(--text-3);
    flex-wrap: wrap;
  }
  .sm-chip {
    height: 17px;
    font-size: 10px;
    padding: 0 6px;
  }
  .ext-busy {
    color: var(--accent);
    display: grid;
    padding: 0 10px;
  }
  .more {
    display: flex;
    justify-content: center;
    padding: 20px;
  }
  .hint-top {
    margin-bottom: 14px;
    max-width: 720px;
  }
  .repos {
    display: flex;
    flex-direction: column;
    gap: 8px;
    max-width: 820px;
  }
  .repo {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 8px 8px 14px;
  }
  .repo-add {
    display: flex;
    gap: 8px;
    margin-top: 6px;
  }
  @media (max-width: 759px) {
    .hide-phone {
      display: none;
    }
    .exts {
      grid-template-columns: 1fr;
    }
  }
</style>
