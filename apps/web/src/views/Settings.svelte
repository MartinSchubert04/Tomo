<script lang="ts">
  // Ajustes: apariencia (idioma, tema, paleta, reducir animaciones), lectura (los valores por
  // defecto del lector en este dispositivo), fuentes (fijadas, NSFW) y la cuenta (contraseña,
  // salir). Las secciones recuerdan si estan abiertas.
  import { BookOpenText, KeyRound, LogOut, Palette, Puzzle, UserRound } from "@lucide/svelte";
  import Section from "../components/Section.svelte";
  import Seg from "../components/Seg.svelte";
  import Switch from "../components/Switch.svelte";
  import { api, errText } from "../lib/api";
  import { brand } from "../lib/brand";
  import { i18n, setLang, t, UI_LANGS, type Lang } from "../lib/i18n.svelte";
  import { bookPrefs, mangaPrefs, saveBookPrefs, saveMangaPrefs, type ReadDir, type ReadMode } from "../lib/readerPrefs.svelte";
  import { applyReduceMotion, logout, session, setPrefs } from "../lib/session.svelte";
  import { isPinned, loadSources, orderedSources, sources, togglePin } from "../lib/sources.svelte";
  import { allPalettes, applyTheme, currentPalette, look, palettesOf, THEMES, type ThemeId } from "../lib/themes.svelte";
  import { toast } from "../lib/toast.svelte";

  loadSources().catch(() => {});

  const theme = $derived(look.theme);
  const palette = $derived(currentPalette(look.theme));
  const swatchOf = (th: ThemeId) => palettesOf(th).find((p) => p.id === currentPalette(th))?.swatch ?? ["#000", "#000", "#000"];
  const themeName = $derived(THEMES.find((th) => th.id === theme)?.name ?? "");
  const paletteName = $derived(palettesOf(theme).find((p) => p.id === palette)?.name ?? "");

  function pickTheme(th: ThemeId) {
    const go = () => {
      applyTheme(th);
      setPrefs({ theme: th, palettes: allPalettes() });
    };
    // Cambio de tema con un fundido de toda la pantalla (si el navegador lo soporta).
    if (document.startViewTransition && !document.documentElement.hasAttribute("data-reduce-motion")) document.startViewTransition(go);
    else go();
  }
  function pickPalette(p: string) {
    applyTheme(theme, p);
    setPrefs({ palettes: allPalettes() });
  }
  function pickLang(l: Lang) {
    setLang(l);
    setPrefs({ lang: l });
  }

  let current = $state("");
  let next = $state("");
  let busy = $state(false);
  async function changePassword(e: Event) {
    e.preventDefault();
    if (next.length < 6) return toast(t("settings.passwordShort"), true);
    busy = true;
    try {
      await api.changePassword(current, next);
      current = next = "";
      toast(t("settings.passwordChanged"));
    } catch (err) {
      toast(errText(err), true);
    } finally {
      busy = false;
    }
  }

  const pinned = $derived((sources.list ? orderedSources(sources.list) : []).filter((s) => isPinned(s.id)));
</script>

<div class="view">
  <div class="page-head"><h1>{t("nav.settings")}</h1></div>
  <div class="sections">
    <Section id="appearance" title={t("settings.look")} icon={Palette} summary="{themeName} · {paletteName}">
      <div class="field">
        {t("common.uiLanguage")}
        <Seg value={i18n.lang} options={UI_LANGS} onchange={(v) => pickLang(v)} />
      </div>
      <div class="sub">{t("settings.theme")}</div>
      <div class="themes">
        {#each THEMES as th, i (th.id)}
          {@const sw = swatchOf(th.id)}
          <button class="theme card stagger" style="--i: {i}" class:on={theme === th.id} onclick={() => pickTheme(th.id)}>
            <div class="sw" style="background: linear-gradient(135deg, {sw[0]} 0 45%, {sw[1]} 45% 75%, {sw[2]} 75%)">
              <span class="mini-cover" style="background: {sw[1]}; border-color: {sw[2]}"></span>
              <span class="mini-cover" style="background: {sw[0]}; border-color: {sw[2]}"></span>
              <span class="mini-bar" style="background: {sw[2]}"></span>
            </div>
            <span>{th.name}</span>
          </button>
        {/each}
      </div>
      <div class="sub">{t("settings.paletteFor", { theme: themeName })}</div>
      <div class="palettes">
        {#each palettesOf(theme) as p (p.id)}
          <button class="pal" class:on={palette === p.id} onclick={() => pickPalette(p.id)}>
            <span class="dots">{#each p.swatch as c, j (j)}<span style="background: {c}"></span>{/each}</span>
            {p.name}
          </button>
        {/each}
      </div>
      <Switch
        checked={!!session.prefs.reduceMotion}
        onchange={(v) => {
          applyReduceMotion(v);
          setPrefs({ reduceMotion: v });
        }}>{t("settings.reduceMotion")}</Switch
      >
    </Section>

    <Section id="reading" title={t("settings.reading")} icon={BookOpenText} summary={t(`reader.mode.${mangaPrefs.mode}`)}>
      <div class="hint-top faint">{t("settings.readingHint")}</div>
      <div class="grid2">
        <div class="field">
          {t("reader.mode")}
          <Seg
            value={mangaPrefs.mode}
            options={[
              { value: "cascade" as ReadMode, label: t("reader.mode.cascade") },
              { value: "paged" as ReadMode, label: t("reader.mode.paged") },
              { value: "double" as ReadMode, label: t("reader.mode.double") },
            ]}
            onchange={(v) => ((mangaPrefs.mode = v), saveMangaPrefs())}
          />
        </div>
        <div class="field">
          {t("reader.direction")}
          <Seg
            value={mangaPrefs.dir}
            options={[
              { value: "rtl" as ReadDir, label: t("reader.dir.rtl") },
              { value: "ltr" as ReadDir, label: t("reader.dir.ltr") },
            ]}
            onchange={(v) => ((mangaPrefs.dir = v), saveMangaPrefs())}
          />
        </div>
      </div>
      <Switch checked={mangaPrefs.autoNext} onchange={(v) => ((mangaPrefs.autoNext = v), saveMangaPrefs())}>{t("reader.autoNext")}</Switch>
      <div class="field">
        {t("settings.bookFlow")}
        <Seg
          value={bookPrefs.flow}
          options={[
            { value: "paginated" as const, label: t("book.flow.paginated") },
            { value: "scrolled" as const, label: t("book.flow.scrolled") },
          ]}
          onchange={(v) => ((bookPrefs.flow = v), saveBookPrefs())}
        />
      </div>
    </Section>

    <Section id="sources" title={t("settings.sources")} icon={Puzzle} summary={t("settings.pinnedN", { n: pinned.length })}>
      <div class="hint-top faint">{t("settings.sourcesHint")}</div>
      {#if sources.list?.length}
        <div class="srcs">
          {#each orderedSources(sources.list) as s (s.id)}
            <button class="pick" class:on={isPinned(s.id)} onclick={() => togglePin(s.id)}>
              <img src={s.icon} alt="" loading="lazy" />{s.name}
            </button>
          {/each}
        </div>
      {:else if sources.list}
        <div class="faint">{t("home.noSources")}</div>
      {/if}
      <Switch checked={!!session.prefs.nsfw} onchange={(v) => setPrefs({ nsfw: v })}>{t("settings.nsfw")}</Switch>
    </Section>

    <Section id="account" title={t("settings.account")} icon={UserRound} summary={session.user?.name ?? ""}>
      <form class="pw" onsubmit={changePassword}>
        <div class="sub"><KeyRound size={13} />{t("settings.changePassword")}</div>
        <div class="grid2">
          <label class="field">{t("settings.currentPassword")}<input type="password" bind:value={current} autocomplete="current-password" /></label>
          <label class="field">{t("settings.newPassword")}<input type="password" bind:value={next} autocomplete="new-password" /></label>
        </div>
        <div><button class="btn primary" disabled={busy || !current || !next}>{t("settings.savePassword")}</button></div>
      </form>
      <div><button class="btn danger" onclick={logout}><LogOut size={15} />{t("nav.logout")}</button></div>
    </Section>
  </div>
  <div class="about faint">{brand.name}</div>
</div>

<style>
  .sections {
    display: flex;
    flex-direction: column;
    gap: 10px;
    max-width: 980px;
  }
  .sub {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 12px;
    font-weight: 600;
    color: var(--text-3);
    text-transform: uppercase;
    letter-spacing: 0.06em;
    margin-bottom: -6px;
  }
  .hint-top {
    font-size: 12.5px;
    margin-top: -4px;
  }
  .themes {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(140px, 1fr));
    gap: 12px;
  }
  .theme {
    padding: 8px;
    display: flex;
    flex-direction: column;
    gap: 8px;
    text-align: left;
    font-weight: 600;
    transition:
      transform 0.2s cubic-bezier(0.2, 0.8, 0.2, 1),
      outline-color 0.2s;
  }
  .theme:hover {
    transform: translateY(-2px);
  }
  .theme.on {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }
  .sw {
    position: relative;
    height: 64px;
    border-radius: var(--radius-sm);
    border: 1px solid color-mix(in srgb, var(--text) 18%, transparent);
    overflow: hidden;
  }
  .mini-cover {
    position: absolute;
    bottom: 8px;
    width: 18px;
    height: 27px;
    border-radius: 3px;
    border: 1.5px solid;
  }
  .mini-cover:nth-child(1) {
    left: 10px;
  }
  .mini-cover:nth-child(2) {
    left: 33px;
  }
  .mini-bar {
    position: absolute;
    left: 10px;
    top: 9px;
    width: 40%;
    height: 5px;
    border-radius: 3px;
  }
  .palettes {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }
  .pal {
    display: inline-flex;
    align-items: center;
    gap: 9px;
    height: 34px;
    padding: 0 13px 0 8px;
    border-radius: 999px;
    background: var(--panel);
    border: var(--border) solid var(--line);
    font-weight: 550;
    transition:
      border-color 0.15s,
      transform 0.15s;
  }
  .pal:hover {
    border-color: var(--line-strong);
    transform: translateY(-1px);
  }
  .pal.on {
    border-color: var(--accent);
    box-shadow: 0 0 0 1px var(--accent);
  }
  .dots {
    display: inline-flex;
    padding-left: 5px;
  }
  .dots span {
    width: 16px;
    height: 16px;
    border-radius: 50%;
    border: 1.5px solid var(--panel);
    margin-left: -5px;
  }
  .srcs {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }
  .srcs img {
    width: 18px;
    height: 18px;
    border-radius: 5px;
  }
  .pw {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  .about {
    margin-top: 24px;
    font-size: 12px;
  }
</style>
