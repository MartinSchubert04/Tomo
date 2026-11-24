<script lang="ts">
  // Ajustes de lectura de libros (EPUB, MOBI, FB2, TXT): letra, tamaño, interlineado, margenes,
  // justificado, paginado o scroll y el color de la pagina. Quedan en este dispositivo.
  import { Minus, Plus } from "@lucide/svelte";
  import Drawer from "./Drawer.svelte";
  import Seg from "./Seg.svelte";
  import Switch from "./Switch.svelte";
  import { t } from "../lib/i18n.svelte";
  import { bookPrefs, PAGE_COLORS, saveBookPrefs, type BookFont, type PageColor } from "../lib/readerPrefs.svelte";

  let { onclose, flow = true }: { onclose: () => void; /** Mostrar "Paginado / Scroll". */ flow?: boolean } = $props();

  function set<K extends keyof typeof bookPrefs>(k: K, v: (typeof bookPrefs)[K]) {
    bookPrefs[k] = v;
    saveBookPrefs();
  }
  const clamp = (v: number, a: number, b: number) => Math.min(b, Math.max(a, v));
</script>

<Drawer title={t("reader.settings")} media {onclose}>
  <div class="field">
    {t("book.font")}
    <Seg
      value={bookPrefs.font}
      options={[
        { value: "original" as BookFont, label: t("book.font.original") },
        { value: "literata" as BookFont, label: "Literata" },
        { value: "inter" as BookFont, label: "Inter" },
      ]}
      onchange={(v) => set("font", v)}
    />
  </div>
  <div class="field">
    {t("book.size")}
    <div class="stepper">
      <button class="btn" onclick={() => set("size", clamp(bookPrefs.size - 10, 60, 220))} aria-label={t("book.smaller")}><Minus size={16} /></button>
      <span class="mono val">{bookPrefs.size} %</span>
      <button class="btn" onclick={() => set("size", clamp(bookPrefs.size + 10, 60, 220))} aria-label={t("book.bigger")}><Plus size={16} /></button>
    </div>
  </div>
  <label class="field">
    <span class="row">{t("book.lineHeight")}<span class="grow"></span><span class="mono">{bookPrefs.lineHeight.toFixed(1)}</span></span>
    <input type="range" min="1.1" max="2.2" step="0.1" value={bookPrefs.lineHeight} oninput={(e) => set("lineHeight", Number(e.currentTarget.value))} />
  </label>
  <label class="field">
    <span class="row">{t("book.margins")}<span class="grow"></span><span class="mono">{bookPrefs.margin} px</span></span>
    <input type="range" min="0" max="120" step="4" value={bookPrefs.margin} oninput={(e) => set("margin", Number(e.currentTarget.value))} />
  </label>
  {#if flow}
    <div class="field">
      {t("book.flow")}
      <Seg
        value={bookPrefs.flow}
        options={[
          { value: "paginated" as const, label: t("book.flow.paginated") },
          { value: "scrolled" as const, label: t("book.flow.scrolled") },
        ]}
        onchange={(v) => set("flow", v)}
      />
    </div>
  {/if}
  <div class="field">
    {t("reader.background")}
    <div class="swatches">
      {#each Object.entries(PAGE_COLORS) as [id, c] (id)}
        <button
          class="sw"
          class:on={bookPrefs.color === id}
          style="background: {c.bg}; color: {c.fg}"
          onclick={() => set("color", id as PageColor)}
          title={t(`reader.bg.${id as PageColor}`)}
          aria-label={t(`reader.bg.${id as PageColor}`)}>Aa</button
        >
      {/each}
    </div>
  </div>
  <Switch checked={bookPrefs.justify} onchange={(v) => set("justify", v)}>{t("book.justify")}</Switch>
</Drawer>

<style>
  .stepper {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .stepper .btn {
    width: 44px;
    height: 38px;
    padding: 0;
  }
  .val {
    min-width: 64px;
    text-align: center;
    font-size: 14px;
  }
  .swatches {
    display: flex;
    gap: 10px;
  }
  .sw {
    width: 46px;
    height: 46px;
    border-radius: 12px;
    border: 2px solid var(--media-line);
    font-family: "Literata Variable", Georgia, serif;
    font-size: 16px;
    font-weight: 600;
    transition: transform 0.15s cubic-bezier(0.34, 1.56, 0.64, 1);
  }
  .sw:hover {
    transform: scale(1.06);
  }
  .sw.on {
    border-color: var(--accent);
    box-shadow: 0 0 0 3px color-mix(in srgb, var(--accent) 35%, transparent);
  }
</style>
