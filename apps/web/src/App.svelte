<script lang="ts">
  // El cascaron: login si no hay sesion; si hay, barra lateral (PC) o pestañas abajo (celular),
  // la barra de arriba con la busqueda y la vista actual. Los lectores ocupan toda la pantalla.
  import {
    BookOpen,
    ChevronsLeft,
    Compass,
    History,
    House,
    LibraryBig,
    LogOut,
    Menu,
    RefreshCw,
    Search,
    Settings,
    Shield,
    WifiOff,
  } from "@lucide/svelte";
  import { flip } from "svelte/animate";
  import { fade } from "svelte/transition";
  import Confirm from "./components/Confirm.svelte";
  import Logo from "./components/Logo.svelte";
  import { confirmState } from "./lib/confirm.svelte";
  import { t } from "./lib/i18n.svelte";
  import { flipMs, ms, rise, toastIn } from "./lib/motion";
  import { net } from "./lib/net.svelte";
  import { FULLSCREEN, go, loadView, route, viewComponent, type ViewName } from "./lib/router.svelte";
  import { logout, restore, session } from "./lib/session.svelte";
  import { read, write } from "./lib/store";
  import { toasts } from "./lib/toast.svelte";
  import Login from "./views/Login.svelte";

  restore().then(() => loadView(route.view));

  const NAV: { view: ViewName; path: string; icon: typeof House; key: Parameters<typeof t>[0] }[] = [
    { view: "home", path: "/", icon: House, key: "nav.home" },
    { view: "explore", path: "/explore", icon: Compass, key: "nav.explore" },
    { view: "library", path: "/library", icon: LibraryBig, key: "nav.library" },
    { view: "books", path: "/books", icon: BookOpen, key: "nav.books" },
    { view: "history", path: "/history", icon: History, key: "nav.history" },
  ];

  let collapsed = $state(read("library.sidebar") === "1");
  let moreOpen = $state(false);
  const full = $derived(FULLSCREEN.includes(route.view));
  const View = $derived(viewComponent(route.view));

  // Cada vista recuerda donde estaba el scroll (al volver con "atras" queda igual).
  let content = $state<HTMLElement>();
  const scrolls = new Map<string, number>();
  let lastKey = "";
  $effect.pre(() => {
    const k = route.path + "?" + route.query.toString();
    if (lastKey && content) scrolls.set(lastKey, content.scrollTop);
    lastKey = k;
  });
  $effect(() => {
    void route.key;
    const k = route.path + "?" + route.query.toString();
    if (!content) return;
    const top = route.pop ? (scrolls.get(k) ?? 0) : 0;
    requestAnimationFrame(() => content?.scrollTo({ top }));
  });

  function nav(e: MouseEvent, path: string) {
    if (e.ctrlKey || e.metaKey || e.button !== 0) return;
    e.preventDefault();
    moreOpen = false;
    go(path);
  }

  // "/" o Ctrl K: a la busqueda.
  function onKey(e: KeyboardEvent) {
    if (!session.user || full) return;
    const typing = (e.target as HTMLElement)?.closest?.("input, textarea, [contenteditable]");
    if ((e.key === "/" && !typing) || ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "k")) {
      e.preventDefault();
      if (route.view === "search") document.querySelector<HTMLInputElement>(".search-input")?.focus();
      else go("/search");
    }
  }

  const active = (v: ViewName) => route.view === v || (v === "explore" && route.view === "manga");
</script>

<svelte:window onkeydown={onKey} />

{#if !session.ready}
  <div class="splash">
    <Logo size={56} shine />
    {#if net.down}<span class="splash-msg"><WifiOff size={15} />{t("common.reconnecting")}</span>{/if}
  </div>
{:else if !session.user}
  <Login />
{:else}
  <div class="shell" class:full class:collapsed>
    {#if !full}
      <aside class="sidebar">
        <div class="brand">
          <a href="#/" onclick={(e) => nav(e, "/")} class="brand-link"><Logo size={26} word={!collapsed} /></a>
        </div>
        <nav>
          {#each NAV as n (n.view)}
            <a
              href={"#" + n.path}
              class="nav-item"
              class:on={active(n.view)}
              onclick={(e) => nav(e, n.path)}
              onpointerenter={() => loadView(n.view)}
              title={collapsed ? t(n.key) : undefined}
            >
              <n.icon size={18} />
              {#if !collapsed}<span>{t(n.key)}</span>{/if}
            </a>
          {/each}
        </nav>
        <span class="grow"></span>
        <nav>
          <a href="#/settings" class="nav-item" class:on={route.view === "settings"} onclick={(e) => nav(e, "/settings")} title={collapsed ? t("nav.settings") : undefined}>
            <Settings size={18} />{#if !collapsed}<span>{t("nav.settings")}</span>{/if}
          </a>
          {#if session.user.admin}
            <a href="#/admin" class="nav-item" class:on={route.view === "admin"} onclick={(e) => nav(e, "/admin")} title={collapsed ? t("nav.admin") : undefined}>
              <Shield size={18} />{#if !collapsed}<span>{t("nav.admin")}</span>{/if}
            </a>
          {/if}
        </nav>
        <div class="me">
          <span class="avatar">{session.user.name.slice(0, 1).toUpperCase()}</span>
          {#if !collapsed}<span class="ellipsis grow">{session.user.name}</span>{/if}
          {#if !collapsed}
            <button class="icon-btn" title={t("nav.logout")} onclick={logout}><LogOut size={15} /></button>
          {/if}
        </div>
        <button
          class="collapse icon-btn"
          title={collapsed ? t("nav.expand") : t("nav.collapse")}
          onclick={() => {
            collapsed = !collapsed;
            write("library.sidebar", collapsed ? "1" : null);
          }}><ChevronsLeft size={16} /></button
        >
      </aside>
    {/if}

    <main class="main">
      {#if !full}
        <header class="topbar">
          <a href="#/" class="phone-brand" onclick={(e) => nav(e, "/")}><Logo size={24} word /></a>
          <a href="#/search" class="searchbox" onclick={(e) => nav(e, "/search")} class:on={route.view === "search"}>
            <Search size={16} />
            <span class="ellipsis">{t("search.placeholder")}</span>
            <kbd>/</kbd>
          </a>
          <a href="#/search" class="icon-btn phone-search" onclick={(e) => nav(e, "/search")} aria-label={t("nav.search")}><Search size={20} /></a>
        </header>
      {/if}
      {#if net.down}
        <div class="netbar" transition:fade={{ duration: ms(150) }}>
          <WifiOff size={15} />
          <span>{t("common.reconnecting")}</span>
          <RefreshCw size={14} class="spin" />
        </div>
      {/if}
      <div class="content" class:full bind:this={content}>
        {#if View}
          {#key route.key + ":" + net.back}
            <div class="view-host" in:rise={{ y: 6, skip: route.view === "reader" || route.view === "book" }}>
              <View />
            </div>
          {/key}
        {:else}
          <div class="splash inline"><Logo size={40} shine /></div>
        {/if}
      </div>
    </main>

    {#if !full}
      <nav class="tabbar">
        {#each NAV.slice(0, 4) as n (n.view)}
          <a href={"#" + n.path} class="tab" class:on={active(n.view)} onclick={(e) => nav(e, n.path)}>
            <n.icon size={21} />
            <span>{t(n.key)}</span>
          </a>
        {/each}
        <button class="tab" class:on={moreOpen || ["history", "settings", "admin"].includes(route.view)} onclick={() => (moreOpen = !moreOpen)}>
          <Menu size={21} />
          <span>{t("nav.more")}</span>
        </button>
      </nav>
      {#if moreOpen}
        <div class="more-scrim" role="presentation" onclick={() => (moreOpen = false)} transition:fade={{ duration: ms(140) }}></div>
        <div class="more-sheet card" in:rise={{ y: 16 }} out:fade={{ duration: ms(120) }}>
          <a href="#/history" class="nav-item" onclick={(e) => nav(e, "/history")}><History size={18} /><span>{t("nav.history")}</span></a>
          <a href="#/settings" class="nav-item" onclick={(e) => nav(e, "/settings")}><Settings size={18} /><span>{t("nav.settings")}</span></a>
          {#if session.user.admin}
            <a href="#/admin" class="nav-item" onclick={(e) => nav(e, "/admin")}><Shield size={18} /><span>{t("nav.admin")}</span></a>
          {/if}
          <button class="nav-item" onclick={logout}><LogOut size={18} /><span>{t("nav.logout")} · {session.user.name}</span></button>
        </div>
      {/if}
    {/if}
  </div>
{/if}

<div class="toasts" class:raised={session.user && !full}>
  {#each toasts as tt (tt.id)}
    <div class="toast card" class:bad={tt.bad} in:toastIn out:fade={{ duration: ms(150) }} animate:flip={{ duration: flipMs() }}>{tt.text}</div>
  {/each}
</div>
{#if confirmState.open}<Confirm />{/if}

<style>
  .splash {
    position: fixed;
    inset: 0;
    display: grid;
    place-items: center;
  }
  .splash {
    align-content: center;
    gap: 18px;
  }
  .splash-msg {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    color: var(--text-2);
    font-size: 13px;
  }
  .splash.inline {
    position: static;
    height: 60vh;
  }
  .shell {
    display: flex;
    height: 100dvh;
  }
  .sidebar {
    position: relative;
    width: 220px;
    flex: none;
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 14px 10px 12px;
    background: var(--sidebar);
    border-right: var(--border) solid var(--line);
    backdrop-filter: var(--blur);
    transition: width 0.22s cubic-bezier(0.2, 0.8, 0.2, 1);
    z-index: 10;
  }
  .collapsed .sidebar {
    width: 64px;
  }
  .brand {
    height: 40px;
    display: flex;
    align-items: center;
    padding: 0 8px;
    margin-bottom: 10px;
  }
  .brand-link {
    display: inline-flex;
  }
  .sidebar nav {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .nav-item {
    position: relative;
    display: flex;
    align-items: center;
    gap: 12px;
    height: 38px;
    padding: 0 12px;
    border-radius: var(--radius-sm);
    color: var(--text-2);
    font-weight: 560;
    white-space: nowrap;
    overflow: hidden;
    transition:
      background 0.15s,
      color 0.15s;
  }
  .nav-item :global(svg) {
    flex: none;
    transition: transform 0.2s cubic-bezier(0.34, 1.56, 0.64, 1);
  }
  .nav-item:hover {
    background: var(--hover);
    color: var(--text);
  }
  .nav-item:hover :global(svg) {
    transform: scale(1.1);
  }
  .nav-item.on {
    background: color-mix(in srgb, var(--accent) 13%, transparent);
    color: var(--text);
  }
  .nav-item.on :global(svg) {
    color: var(--accent);
  }
  .nav-item.on::before {
    content: "";
    position: absolute;
    left: 0;
    top: 9px;
    bottom: 9px;
    width: 3px;
    border-radius: 3px;
    background: var(--accent);
  }
  :global([data-theme="poolsuite"]) .nav-item.on {
    background: var(--accent);
    color: var(--accent-ink);
    box-shadow: 2px 2px 0 var(--line);
  }
  :global([data-theme="poolsuite"]) .nav-item.on :global(svg) {
    color: var(--accent-ink);
  }
  .me {
    display: flex;
    align-items: center;
    gap: 10px;
    margin-top: 8px;
    padding: 10px 6px 0;
    border-top: var(--border) solid var(--line);
    font-weight: 560;
    min-width: 0;
  }
  .avatar {
    width: 30px;
    height: 30px;
    flex: none;
    display: grid;
    place-items: center;
    border-radius: 50%;
    background: var(--primary-bg);
    color: var(--accent-ink);
    font-weight: 750;
    font-size: 13px;
  }
  .collapse {
    position: absolute;
    top: 20px;
    right: -14px;
    width: 26px;
    height: 26px;
    border-radius: 50%;
    background: var(--panel);
    border: var(--border) solid var(--line-strong);
    opacity: 0;
    transition:
      opacity 0.15s,
      transform 0.22s;
  }
  .sidebar:hover .collapse {
    opacity: 1;
  }
  .collapsed .collapse {
    transform: rotate(180deg);
  }
  .main {
    position: relative;
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
  }
  .topbar {
    display: flex;
    align-items: center;
    gap: 10px;
    height: 58px;
    flex: none;
    padding: 0 var(--gutter);
  }
  .searchbox {
    display: flex;
    align-items: center;
    gap: 10px;
    width: min(460px, 100%);
    height: 38px;
    padding: 0 12px;
    border-radius: 999px;
    background: var(--panel-2);
    border: var(--border) solid var(--line);
    color: var(--text-3);
    transition:
      border-color 0.15s,
      box-shadow 0.15s;
  }
  .searchbox span {
    flex: 1;
  }
  .searchbox:hover {
    border-color: var(--line-strong);
  }
  .searchbox.on {
    display: none;
  }
  .phone-brand,
  .phone-search {
    display: none;
  }
  .content {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    overflow-x: hidden;
    overscroll-behavior: contain;
  }
  .content.full {
    overflow: hidden;
  }
  .view-host {
    min-height: 100%;
  }
  .content.full .view-host {
    height: 100%;
  }
  .netbar {
    position: absolute;
    top: 10px;
    left: 50%;
    transform: translateX(-50%);
    z-index: 50;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 7px 14px;
    border-radius: 999px;
    background: var(--danger);
    color: #fff;
    font-weight: 600;
    font-size: 12.5px;
    box-shadow: var(--pop-shadow);
    white-space: nowrap;
  }
  .tabbar,
  .more-sheet,
  .more-scrim {
    display: none;
  }
  .toasts {
    position: fixed;
    right: 18px;
    bottom: 18px;
    z-index: 100;
    display: flex;
    flex-direction: column;
    align-items: flex-end;
    gap: 8px;
    pointer-events: none;
  }
  .toast {
    padding: 10px 14px;
    background: var(--float, var(--sidebar));
    box-shadow: var(--pop-shadow);
    font-weight: 550;
    max-width: min(380px, calc(100vw - 36px));
    pointer-events: auto;
  }
  .toast.bad {
    border-color: var(--danger);
    color: var(--danger);
  }

  @media (max-width: 759px) {
    .sidebar {
      display: none;
    }
    .topbar {
      height: 54px;
      padding-top: env(safe-area-inset-top);
      height: calc(54px + env(safe-area-inset-top));
      justify-content: space-between;
    }
    .phone-brand {
      display: inline-flex;
    }
    .searchbox {
      display: none;
    }
    .phone-search {
      display: inline-grid;
      width: 40px;
      height: 40px;
    }
    .tabbar {
      position: fixed;
      left: 0;
      right: 0;
      bottom: 0;
      z-index: 30;
      display: flex;
      height: calc(var(--tabbar-h) + env(safe-area-inset-bottom));
      padding-bottom: env(safe-area-inset-bottom);
      background: color-mix(in srgb, var(--sidebar) 92%, transparent);
      backdrop-filter: blur(16px) saturate(140%);
      border-top: var(--border) solid var(--line);
    }
    .tab {
      flex: 1;
      display: flex;
      flex-direction: column;
      align-items: center;
      justify-content: center;
      gap: 3px;
      color: var(--text-3);
      font-size: 10.5px;
      font-weight: 600;
      -webkit-tap-highlight-color: transparent;
      transition: color 0.15s;
    }
    .tab :global(svg) {
      transition: transform 0.25s cubic-bezier(0.34, 1.56, 0.64, 1);
    }
    .tab.on {
      color: var(--accent);
    }
    .tab.on :global(svg) {
      transform: translateY(-1px) scale(1.08);
    }
    .tab:active :global(svg) {
      transform: scale(0.88);
    }
    .more-scrim {
      display: block;
      position: fixed;
      inset: 0;
      z-index: 28;
      background: rgba(0, 0, 0, 0.35);
    }
    .more-sheet {
      display: flex;
      flex-direction: column;
      gap: 2px;
      position: fixed;
      right: 10px;
      left: 10px;
      bottom: calc(var(--tabbar-h) + env(safe-area-inset-bottom) + 8px);
      z-index: 29;
      padding: 8px;
      background: var(--float, var(--sidebar));
      box-shadow: var(--pop-shadow);
    }
    .more-sheet .nav-item {
      height: 46px;
      font-size: 14.5px;
    }
    .toasts {
      left: 12px;
      right: 12px;
      bottom: 12px;
      align-items: stretch;
    }
    .toasts.raised {
      bottom: calc(var(--tabbar-h) + env(safe-area-inset-bottom) + 10px);
    }
  }
</style>
