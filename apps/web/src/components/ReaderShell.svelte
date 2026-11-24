<script lang="ts" module>
  export type PickItem = { id: string; label: string; sub?: string; done?: boolean };

  /** El capitulo actual del selector queda centrado al abrirlo. */
  function scrollIfCurrent(node: HTMLElement, on: boolean) {
    if (on) requestAnimationFrame(() => node.scrollIntoView({ block: "center" }));
  }
</script>

<script lang="ts">
  // El lector de paginas (manga, CBZ y PDF). La pagina es la protagonista: fondo del color elegido
  // de borde a borde y las barras escondidas. Modos:
  //  - cascada: tira vertical; los capitulos siguientes se agregan abajo (lectura infinita)
  //  - paginado: una pagina, se pasa deslizando o tocando los costados (derecha a izquierda o al reves)
  //  - doble: dos paginas lado a lado (pantallas anchas), la primera sola (la tapa)
  // Cada pagina reserva su alto (relacion de aspecto) asi la tira no salta; se cargan las que estan
  // cerca (≈3 adelante). Teclado: ←/→, A/D, espacio, RePag/AvPag, Inicio/Fin, [ ], F, M, Esc.
  import {
    ArrowLeft,
    ChevronDown,
    ChevronLeft,
    ChevronRight,
    List,
    LoaderCircle,
    Maximize,
    RefreshCw,
    Settings2,
    SkipBack,
    SkipForward,
  } from "@lucide/svelte";
  import { onMount, tick, untrack } from "svelte";
  import { fade } from "svelte/transition";
  import Drawer from "./Drawer.svelte";
  import Seg from "./Seg.svelte";
  import Switch from "./Switch.svelte";
  import { t } from "../lib/i18n.svelte";
  import { barBottom, barTop, ms, reduceMotion } from "../lib/motion";
  import { DEFAULT_RATIO, type PageItem, type Segment } from "../lib/pages";
  import { mangaPrefs, PAGE_COLORS, saveMangaPrefs, type Fit, type PageColor, type ReadDir, type ReadMode } from "../lib/readerPrefs.svelte";

  let {
    title,
    ontitle,
    segments,
    start = { seg: 0, page: 0 },
    onback,
    onposition,
    onnearend,
    onprev,
    onnext,
    hasPrev = false,
    hasNext = false,
    loadingNext = false,
    nextLabel = "",
    chapters,
    currentChapter = "",
    onpick,
    allowDouble = true,
    ltr = false,
  }: {
    title: string;
    /** Tocar el titulo (manga: a la ficha; libro: a sus datos). */
    ontitle?: () => void;
    segments: Segment[];
    start?: { seg: number; page: number };
    onback: () => void;
    /** Cambio la pagina visible (segmento, pagina, total de paginas del segmento). */
    onposition?: (seg: number, page: number, count: number) => void;
    /** En cascada, cerca del final del ultimo segmento: el padre puede agregar el siguiente. */
    onnearend?: () => void;
    onprev?: () => void;
    onnext?: () => void;
    hasPrev?: boolean;
    hasNext?: boolean;
    loadingNext?: boolean;
    /** "Cap. 13" para la tarjeta del final. */
    nextLabel?: string;
    /** Para el selector de capitulos (manga). */
    chapters?: PickItem[];
    currentChapter?: string;
    onpick?: (id: string) => void;
    allowDouble?: boolean;
    /** Siempre de izquierda a derecha (PDF), sin importar el ajuste de manga. */
    ltr?: boolean;
  } = $props();

  // svelte-ignore state_referenced_locally
  let seg = $state(start.seg);
  // svelte-ignore state_referenced_locally
  let page = $state(start.page);
  let chrome = $state(true);
  let settings = $state(false);
  let picker = $state(false);

  const wide = typeof matchMedia !== "undefined" && matchMedia("(min-width: 1000px)").matches;
  const mode = $derived<ReadMode>(mangaPrefs.mode === "double" && (!wide || !allowDouble) ? "paged" : mangaPrefs.mode);
  const colors = $derived(PAGE_COLORS[mangaPrefs.bg]);
  const current = $derived(segments[Math.min(seg, segments.length - 1)]);
  const count = $derived(current?.pages.length ?? 0);

  // Relaciones de aspecto conocidas ("s:p" → ancho/alto) y paginas que ya se pueden pedir.
  let ratios = $state<Record<string, number>>({});
  let wanted = $state<Record<string, true>>({});
  let failed = $state<Record<string, number>>({});
  const key = (s: number, p: number) => `${s}:${p}`;
  const ratioOf = (s: number, p: number, item: PageItem) => ratios[key(s, p)] ?? item.ratio ?? firstRatio(s) ?? DEFAULT_RATIO;
  function firstRatio(s: number): number | undefined {
    for (const k in ratios) if (k.startsWith(s + ":")) return ratios[k];
    return undefined;
  }

  function report() {
    onposition?.(seg, page, count);
  }

  // ---------- barras ----------
  let idle = 0;
  function showChrome(v: boolean) {
    chrome = v;
    clearTimeout(idle);
    if (v) idle = window.setTimeout(() => !settings && !picker && !scrubbing && (chrome = false), 3500);
  }
  onMount(() => {
    showChrome(true);
    return () => clearTimeout(idle);
  });

  // ---------- cascada ----------
  let scroller = $state<HTMLDivElement>();
  let lastY = 0;
  let nearEndAsked = -1;

  /** Ultima vez que el usuario movio la pagina (rueda, dedo, teclado). Los saltos propios y los
   *  ajustes del navegador al cargar imagenes tambien disparan "scroll" y no tienen que esconder
   *  ni mostrar las barras. */
  let userAt = 0;
  const touched = () => (userAt = performance.now());

  function onCascadeScroll() {
    if (!scroller) return;
    const y = scroller.scrollTop;
    // Bajando se esconden las barras; subiendo un poco aparecen.
    if (performance.now() - userAt < 900) {
      if (y > lastY + 12 && chrome && !settings) chrome = false;
      else if (y < lastY - 40 && !chrome) showChrome(true);
    }
    lastY = y;
    // Cerca del final de lo cargado: pedir el capitulo siguiente (una vez por segmento).
    const left = scroller.scrollHeight - (y + scroller.clientHeight);
    if (left < scroller.clientHeight * 3 && nearEndAsked !== segments.length) {
      nearEndAsked = segments.length;
      onnearend?.();
    }
  }

  // Observadores: qué paginas estan cerca (para pedirlas) y cual esta en el medio de la pantalla.
  let nearIO: IntersectionObserver | null = null;
  let midIO: IntersectionObserver | null = null;
  $effect(() => {
    if (mode !== "cascade" || !scroller) return;
    const root = scroller;
    nearIO = new IntersectionObserver(
      (es) => {
        const add: Record<string, true> = {};
        for (const e of es) if (e.isIntersecting) add[(e.target as HTMLElement).dataset.k!] = true;
        if (Object.keys(add).length) wanted = { ...wanted, ...add };
      },
      { root, rootMargin: "100% 0px 250% 0px" },
    );
    midIO = new IntersectionObserver(
      (es) => {
        for (const e of es) {
          if (!e.isIntersecting) continue;
          const el = e.target as HTMLElement;
          const s = Number(el.dataset.s);
          const p = Number(el.dataset.p);
          if (s !== seg || p !== page) {
            seg = s;
            page = p;
            report();
          }
        }
      },
      { root, rootMargin: "-50% 0px -50% 0px" },
    );
    const els = root.querySelectorAll<HTMLElement>(".pg");
    els.forEach((el) => (nearIO!.observe(el), midIO!.observe(el)));
    return () => {
      nearIO?.disconnect();
      midIO?.disconnect();
      nearIO = midIO = null;
    };
  });
  /** Cada pagina se suscribe sola a los observadores al montarse (las de capitulos agregados). */
  function observe(el: HTMLElement) {
    nearIO?.observe(el);
    midIO?.observe(el);
    return { destroy: () => (nearIO?.unobserve(el), midIO?.unobserve(el)) };
  }

  // ---------- rueda y teclado en cascada ----------
  // Desplazamiento propio (Ajustes de lectura → "Desplazamiento de la rueda" / "con teclado"): un
  // animador que va acercando scrollTop a un objetivo. Los pasos seguidos suman al objetivo (con
  // scrollBy smooth cada paso nuevo cortaba al anterior y se perdia distancia).
  let glideTo: number | null = null;
  let glideRaf = 0;
  let glideSet = -1;
  function glide(dy: number, instant = false) {
    const el = scroller;
    if (!el) return;
    touched();
    const max = el.scrollHeight - el.clientHeight;
    // Si el usuario movio la pagina por otro lado (barra, dedo), se arranca desde donde esta.
    const from = glideTo !== null && Math.abs(el.scrollTop - glideSet) < 3 ? glideTo : el.scrollTop;
    glideTo = Math.max(0, Math.min(max, from + dy));
    if (instant || reduceMotion()) {
      el.scrollTop = glideTo;
      glideTo = null;
      return;
    }
    if (!glideRaf) glideRaf = requestAnimationFrame(glideStep);
  }
  function glideStep() {
    glideRaf = 0;
    const el = scroller;
    if (!el || glideTo === null) return;
    // Lo movio otro (arrastrar la barra, el dedo): se suelta.
    if (glideSet >= 0 && Math.abs(el.scrollTop - glideSet) > 3) return void (glideTo = null, (glideSet = -1));
    const d = glideTo - el.scrollTop;
    if (Math.abs(d) < 1) {
      el.scrollTop = glideTo;
      glideTo = null;
      glideSet = -1;
      return;
    }
    // Se acerca un 22 % por cuadro (frena al llegar), al menos 1 px.
    el.scrollTop += Math.sign(d) * Math.max(1, Math.abs(d) * 0.22);
    glideSet = el.scrollTop;
    touched();
    glideRaf = requestAnimationFrame(glideStep);
  }
  $effect(() => () => cancelAnimationFrame(glideRaf));

  /** La rueda en cascada: escala el paso (no salta fijo, asi el touchpad sigue suave). Con 100 %
   *  no se toca nada; Ctrl + rueda (zoom, pellizcar en el touchpad) tampoco. */
  function wheel(node: HTMLElement) {
    const on = (e: WheelEvent) => {
      const k = mangaPrefs.wheelStep / 100;
      if (e.ctrlKey || k === 1 || Math.abs(e.deltaX) > Math.abs(e.deltaY)) return;
      e.preventDefault();
      const dy = e.deltaY * (e.deltaMode === 1 ? 40 : e.deltaMode === 2 ? node.clientHeight : 1) * k;
      // Touchpad: muchos eventos chicos; van directo (ya son suaves). Rueda: animado.
      glide(dy, e.deltaMode === 0 && Math.abs(e.deltaY) < 40);
    };
    node.addEventListener("wheel", on, { passive: false });
    return { destroy: () => node.removeEventListener("wheel", on) };
  }

  async function jumpCascade(s: number, p: number, smooth = false) {
    await tick();
    const el = scroller?.querySelector<HTMLElement>(`.pg[data-k="${key(s, p)}"]`);
    if (!el || !scroller) return;
    wanted = { ...wanted, [key(s, p)]: true, [key(s, p + 1)]: true };
    const top = el.offsetTop - (p === 0 ? 0 : 8);
    // El salto no cuenta como "bajar": las barras quedan como estaban.
    lastY = top;
    scroller.scrollTo({ top, behavior: smooth ? "smooth" : "auto" });
  }

  // ---------- paginado y doble ----------
  /** Pantallas (spreads) del segmento actual: indices de pagina. En doble, la primera va sola. */
  const spreads = $derived.by(() => {
    const n = count;
    const out: number[][] = [];
    if (mode !== "double") for (let i = 0; i < n; i++) out.push([i]);
    else {
      if (n) out.push([0]);
      for (let i = 1; i < n; i += 2) out.push(i + 1 < n ? [i, i + 1] : [i]);
    }
    return out;
  });
  const spreadOf = (p: number) => Math.max(0, spreads.findIndex((sp) => sp.includes(p)));
  // svelte-ignore state_referenced_locally
  let si = $state(0);
  let dragX = $state(0);
  let dragging = $state(false);
  let stripW = $state(1);
  const rtl = $derived(!ltr && mangaPrefs.dir === "rtl");
  /** Una pantalla mas al final: la tarjeta de "Fin del capitulo". */
  const slides = $derived(spreads.length + 1);

  function goSpread(i: number) {
    const n = Math.max(0, Math.min(slides - 1, i));
    si = n;
    const sp = spreads[n];
    if (sp) {
      page = sp[0];
      report();
    } else if (count) {
      // La tarjeta del final cuenta como la ultima pagina leida.
      page = count - 1;
      report();
    }
    const want: Record<string, true> = {};
    for (let k = n - 1; k <= n + 3; k++) for (const p of spreads[k] ?? []) want[key(seg, p)] = true;
    wanted = { ...wanted, ...want };
  }

  function nextPage() {
    if (mode === "cascade") return scroller && glide(scroller.clientHeight * (mangaPrefs.keyStep / 100));
    if (si >= slides - 1) return hasNext && onnext?.();
    goSpread(si + 1);
  }
  function prevPage() {
    if (mode === "cascade") return scroller && glide(-scroller.clientHeight * (mangaPrefs.keyStep / 100));
    if (si <= 0) return hasPrev && onprev?.();
    goSpread(si - 1);
  }

  // Deslizar con el dedo o el mouse (el desplazamiento sigue al dedo y despues se acomoda).
  let x0 = 0;
  let y0 = 0;
  let moved = false;
  let pid = -1;
  function down(e: PointerEvent) {
    if (mode === "cascade" || e.button !== 0) return;
    pid = e.pointerId;
    x0 = e.clientX;
    y0 = e.clientY;
    moved = false;
  }
  function move(e: PointerEvent) {
    if (e.pointerId !== pid || mode === "cascade") return;
    const dx = e.clientX - x0;
    if (!dragging && Math.abs(dx) > 10 && Math.abs(dx) > Math.abs(e.clientY - y0)) {
      dragging = true;
      moved = true;
      (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    }
    if (dragging) dragX = dx;
  }
  function up(e: PointerEvent) {
    if (e.pointerId !== pid) return;
    pid = -1;
    if (!dragging) return;
    const dx = dragX;
    dragging = false;
    dragX = 0;
    const th = Math.min(120, stripW * 0.18);
    // En derecha a izquierda, arrastrar hacia la derecha avanza.
    if (dx < -th) rtl ? prevPage() : nextPage();
    else if (dx > th) rtl ? nextPage() : prevPage();
  }

  // Tocar: tercios de los costados pasan de pagina (al reves en RTL), el centro muestra las barras.
  function tap(e: MouseEvent) {
    if (moved) return (moved = false);
    if ((e.target as HTMLElement).closest("button, a, input")) return;
    const x = e.clientX / innerWidth;
    if (mode === "cascade" || (x > 0.3 && x < 0.7)) return showChrome(!chrome);
    const left = x <= 0.3;
    if (left !== rtl) prevPage();
    else nextPage();
    if (chrome) chrome = false;
  }

  // ---------- al empezar y al cambiar de modo ----------
  let ready = false;
  $effect(() => {
    void mode;
    untrack(() => {
      if (mode === "cascade") jumpCascade(seg, ready ? page : start.page);
      else {
        // svelte-ignore state_referenced_locally
        goSpread(spreadOf(ready ? page : start.page));
      }
      ready = true;
    });
  });

  // En paginado solo se muestra el segmento actual: si llegan segmentos nuevos (cascada) no cambia.

  // ---------- barra de paginas ----------
  let scrubbing = $state(false);
  let scrub = $state(0);
  const shownPage = $derived(scrubbing ? scrub : page);
  function scrubTo(p: number) {
    if (mode === "cascade") jumpCascade(seg, p);
    else goSpread(spreadOf(p));
  }

  // ---------- teclado ----------
  function onKey(e: KeyboardEvent) {
    if (settings || picker || e.ctrlKey || e.metaKey || e.altKey) return;
    if ((e.target as HTMLElement)?.closest?.("input, textarea")) return;
    touched();
    const k = e.key;
    const fwd = () => (e.preventDefault(), nextPage());
    const bwd = () => (e.preventDefault(), prevPage());
    if (k === "ArrowRight" || k === "d" || k === "D") return mode !== "cascade" && rtl ? bwd() : fwd();
    if (k === "ArrowLeft" || k === "a" || k === "A") return mode !== "cascade" && rtl ? fwd() : bwd();
    if (mode === "cascade" && k === "ArrowDown") return fwd();
    if (mode === "cascade" && k === "ArrowUp") return bwd();
    if (k === " " || k === "PageDown") return e.shiftKey ? bwd() : fwd();
    if (k === "PageUp") return bwd();
    if (k === "Home") return (e.preventDefault(), scrubTo(0));
    if (k === "End") return (e.preventDefault(), scrubTo(Math.max(0, count - 1)));
    if (k === "]") return hasNext && onnext?.();
    if (k === "[") return hasPrev && onprev?.();
    if (k === "f" || k === "F") return fullscreen();
    if (k === "m" || k === "M") {
      const order: ReadMode[] = wide && allowDouble ? ["cascade", "paged", "double"] : ["cascade", "paged"];
      setMode(order[(order.indexOf(mode) + 1) % order.length]);
      return;
    }
    if (k === "Escape") return (e.preventDefault(), onback());
  }

  function fullscreen() {
    if (document.fullscreenElement) document.exitFullscreen().catch(() => {});
    else document.documentElement.requestFullscreen?.().catch(() => {});
  }

  function setMode(m: ReadMode) {
    mangaPrefs.mode = m;
    saveMangaPrefs();
  }
  function setPref<K extends keyof typeof mangaPrefs>(k: K, v: (typeof mangaPrefs)[K]) {
    mangaPrefs[k] = v;
    saveMangaPrefs();
  }

  // ---------- imagenes ----------
  function loaded(s: number, p: number, e: Event) {
    const img = e.currentTarget as HTMLImageElement;
    if (img.naturalWidth && img.naturalHeight) ratios = { ...ratios, [key(s, p)]: img.naturalWidth / img.naturalHeight };
  }
  function failedImg(s: number, p: number) {
    failed = { ...failed, [key(s, p)]: (failed[key(s, p)] ?? 0) + 1 };
  }
  function retry(s: number, p: number) {
    const k = key(s, p);
    const n = { ...failed };
    retries[k] = (retries[k] ?? 0) + 1;
    delete n[k];
    failed = n;
  }
  let retries = $state<Record<string, number>>({});
  const srcOf = (item: { src: string }, k: string) => (retries[k] ? `${item.src}${item.src.includes("?") ? "&" : "?"}r=${retries[k]}` : item.src);

  /** Accion para las paginas de PDF: dibuja cuando la pagina es "wanted" y al cambiar el ancho. */
  function draw(canvas: HTMLCanvasElement, o: { item: PageItem; k: string; on: boolean }) {
    let done = 0;
    let busy = false;
    const run = async (opt: typeof o) => {
      if (!opt.on || !("draw" in opt.item) || busy) return;
      const w = canvas.parentElement?.clientWidth ?? 0;
      if (!w || Math.abs(w - done) < 2) return;
      busy = true;
      try {
        const r = await opt.item.draw(canvas, w);
        done = w;
        if (r && Math.abs((ratios[opt.k] ?? 0) - r) > 0.001) ratios = { ...ratios, [opt.k]: r };
      } catch {
        failed = { ...failed, [opt.k]: 1 };
      } finally {
        busy = false;
      }
    };
    run(o);
    const ro = new ResizeObserver(() => run(o));
    if (canvas.parentElement) ro.observe(canvas.parentElement);
    return {
      update(n: typeof o) {
        o = n;
        run(n);
      },
      destroy: () => ro.disconnect(),
    };
  }

  const pagesLabel = $derived(count ? `${shownPage + 1} / ${count}` : "");
</script>

<svelte:window onkeydown={onKey} />

<div
  class="reader"
  class:cascade={mode === "cascade"}
  class:fit-height={mangaPrefs.fit === "height"}
  class:fit-original={mangaPrefs.fit === "original"}
  style="--page-bg: {colors.bg}; --page-fg: {colors.fg}; --maxw: {mangaPrefs.maxWidth}px; --gap: {mangaPrefs.gap}px"
>
  {#if mode === "cascade"}
    <div
      class="scroller"
      bind:this={scroller}
      onscroll={onCascadeScroll}
      use:wheel
      onwheel={touched}
      ontouchmove={touched}
      onkeydown={touched}
      onclick={tap}
      role="presentation"
    >
      {#each segments as sg, s (sg.id)}
        {#if s > 0}
          <div class="divider"><span>{sg.title}</span></div>
        {/if}
        <div class="column">
          {#each sg.pages as item, p (p)}
            {@const k = key(s, p)}
            <div class="pg" data-k={k} data-s={s} data-p={p} style="aspect-ratio: {ratioOf(s, p, item)}" use:observe>
              {#if failed[k]}
                <button class="pg-err" onclick={() => retry(s, p)}><RefreshCw size={18} />{t("reader.pageFailed")}</button>
              {:else if "src" in item}
                {#if wanted[k]}
                  <img src={srcOf(item, k)} alt="" decoding="async" onload={(e) => loaded(s, p, e)} onerror={() => failedImg(s, p)} draggable="false" />
                {/if}
                <span class="pg-n">{p + 1}</span>
              {:else}
                <canvas use:draw={{ item, k, on: !!wanted[k] }}></canvas>
                <span class="pg-n">{p + 1}</span>
              {/if}
            </div>
          {/each}
        </div>
      {/each}
      <div class="end">
        {#if loadingNext}
          <LoaderCircle size={22} class="spin" />
        {:else if hasNext && segments.length && seg === segments.length - 1}
          <p>{t("reader.endOf", { ch: current?.title ?? "" })}</p>
          <button class="btn primary lg" onclick={() => onnext?.()}>{t("reader.next")} · {nextLabel}<SkipForward size={16} /></button>
        {:else if !hasNext}
          <p>{t("reader.theEnd")}</p>
          <button class="btn lg" onclick={onback}>{t("common.back")}</button>
        {/if}
      </div>
    </div>
  {:else}
    <div
      class="strip-host"
      bind:clientWidth={stripW}
      onpointerdown={down}
      onpointermove={move}
      onpointerup={up}
      onpointercancel={up}
      onclick={tap}
      role="presentation"
    >
      <div
        class="strip"
        class:rtl
        class:dragging
        style="transform: translateX({(rtl ? si : -si) * stripW + dragX}px)"
      >
        {#each spreads as sp, i (i)}
          <div class="slide" class:double={sp.length > 1}>
            {#if Math.abs(i - si) <= 3}
              {#each rtl && sp.length > 1 ? [...sp].reverse() : sp as p (p)}
                {@const item = current.pages[p]}
                {@const k = key(seg, p)}
                <div class="sp" class:half={sp.length > 1}>
                  {#if failed[k]}
                    <button class="pg-err" onclick={() => retry(seg, p)}><RefreshCw size={18} />{t("reader.pageFailed")}</button>
                  {:else if "src" in item}
                    {#if wanted[k]}
                      <img src={srcOf(item, k)} alt="" decoding="async" onload={(e) => loaded(seg, p, e)} onerror={() => failedImg(seg, p)} draggable="false" />
                    {:else}
                      <LoaderCircle size={22} class="spin faint-ico" />
                    {/if}
                  {:else}
                    <div class="canvas-host" style="aspect-ratio: {ratioOf(seg, p, item)}; --r: {ratioOf(seg, p, item)}">
                      <canvas use:draw={{ item, k, on: !!wanted[k] }}></canvas>
                    </div>
                  {/if}
                </div>
              {/each}
            {/if}
          </div>
        {/each}
        <div class="slide end">
          {#if hasNext}
            <p>{t("reader.endOf", { ch: current?.title ?? "" })}</p>
            <button class="btn primary lg" onclick={() => onnext?.()}>{t("reader.next")} · {nextLabel}<SkipForward size={16} /></button>
          {:else}
            <p>{t("reader.theEnd")}</p>
            <button class="btn lg" onclick={onback}>{t("common.back")}</button>
          {/if}
        </div>
      </div>
    </div>
  {/if}

  {#if chrome}
    <header class="bar top" transition:barTop>
      <button class="icon-btn" onclick={onback} aria-label={t("common.back")}><ArrowLeft size={20} /></button>
      <div class="titles">
        {#if ontitle}
          <button class="t1 ellipsis link" onclick={ontitle} title={t("reader.toDetail")}>{title}</button>
        {:else}
          <span class="t1 ellipsis">{title}</span>
        {/if}
        {#if current?.title}
          {#if chapters?.length}
            <button class="t2 ellipsis link" onclick={() => (picker = true)} title={t("reader.chapters")}>{current.title}<ChevronDown size={12} /></button>
          {:else}
            <span class="t2 ellipsis">{current.title}</span>
          {/if}
        {/if}
      </div>
      {#if chapters?.length}
        <button class="icon-btn" onclick={() => (picker = true)} title={t("reader.chapters")}><List size={19} /></button>
      {/if}
      <button class="icon-btn hide-phone" onclick={fullscreen} title={t("reader.fullscreen")}><Maximize size={18} /></button>
      <button class="icon-btn" onclick={() => (settings = true)} title={t("reader.settings")}><Settings2 size={19} /></button>
    </header>
    <footer class="bar bottom" transition:barBottom>
      <button class="icon-btn" disabled={!hasPrev} onclick={() => onprev?.()} title={t("reader.prev")}>
        {#if rtl && mode !== "cascade"}<SkipForward size={18} />{:else}<SkipBack size={18} />{/if}
      </button>
      <div class="scrub" class:rtl={rtl && mode !== "cascade"}>
        <input
          type="range"
          min="0"
          max={Math.max(0, count - 1)}
          value={shownPage}
          oninput={(e) => {
            scrubbing = true;
            scrub = Number(e.currentTarget.value);
          }}
          onchange={(e) => {
            scrubbing = false;
            scrubTo(Number(e.currentTarget.value));
          }}
          aria-label={t("reader.page")}
        />
      </div>
      <span class="count mono">{pagesLabel}</span>
      <button class="icon-btn" disabled={!hasNext} onclick={() => onnext?.()} title={t("reader.next")}>
        {#if rtl && mode !== "cascade"}<SkipBack size={18} />{:else}<SkipForward size={18} />{/if}
      </button>
    </footer>
    {#if mode !== "cascade"}
      <button class="side l" onclick={() => (rtl ? nextPage() : prevPage())} aria-label={t("reader.page")} transition:fade={{ duration: ms(150) }}><ChevronLeft size={28} /></button>
      <button class="side r" onclick={() => (rtl ? prevPage() : nextPage())} aria-label={t("reader.page")} transition:fade={{ duration: ms(150) }}><ChevronRight size={28} /></button>
    {/if}
  {/if}
</div>

{#if settings}
  <Drawer title={t("reader.settings")} media onclose={() => ((settings = false), showChrome(true))}>
    <div class="field">
      {t("reader.mode")}
      <Seg
        value={mangaPrefs.mode}
        options={[
          { value: "cascade" as ReadMode, label: t("reader.mode.cascade") },
          { value: "paged" as ReadMode, label: t("reader.mode.paged") },
          ...(wide && allowDouble ? [{ value: "double" as ReadMode, label: t("reader.mode.double") }] : []),
        ]}
        onchange={(v) => setMode(v)}
      />
    </div>
    {#if mode !== "cascade" && !ltr}
      <div class="field">
        {t("reader.direction")}
        <Seg
          value={mangaPrefs.dir}
          options={[
            { value: "rtl" as ReadDir, label: t("reader.dir.rtl") },
            { value: "ltr" as ReadDir, label: t("reader.dir.ltr") },
          ]}
          onchange={(v) => setPref("dir", v)}
        />
      </div>
    {/if}
    <div class="field">
      {t("reader.fit")}
      <Seg
        value={mangaPrefs.fit}
        options={[
          { value: "width" as Fit, label: t("reader.fit.width") },
          { value: "height" as Fit, label: t("reader.fit.height") },
          { value: "original" as Fit, label: t("reader.fit.original") },
        ]}
        onchange={(v) => setPref("fit", v)}
      />
    </div>
    {#if mode === "cascade"}
      <label class="field">
        <span class="row">{t("reader.maxWidth")}<span class="grow"></span><span class="mono">{mangaPrefs.maxWidth} px</span></span>
        <input type="range" min="480" max="1600" step="20" value={mangaPrefs.maxWidth} oninput={(e) => setPref("maxWidth", Number(e.currentTarget.value))} />
      </label>
      <label class="field">
        <span class="row">{t("reader.wheelStep")}<span class="grow"></span><span class="mono">{mangaPrefs.wheelStep} %</span></span>
        <input type="range" min="50" max="400" step="10" value={mangaPrefs.wheelStep} oninput={(e) => setPref("wheelStep", Number(e.currentTarget.value))} />
        <span class="hint">{t("reader.wheelStepHint")}</span>
      </label>
      <label class="field">
        <span class="row">{t("reader.keyStep")}<span class="grow"></span><span class="mono">{mangaPrefs.keyStep} %</span></span>
        <input type="range" min="25" max="100" step="5" value={mangaPrefs.keyStep} oninput={(e) => setPref("keyStep", Number(e.currentTarget.value))} />
        <span class="hint">{t("reader.keyStepHint")}</span>
      </label>
      <div class="field">
        {t("reader.gap")}
        <Seg
          value={mangaPrefs.gap}
          options={[
            { value: 0 as const, label: t("reader.gap.none") },
            { value: 8 as const, label: t("reader.gap.small") },
            { value: 24 as const, label: t("reader.gap.large") },
          ]}
          onchange={(v) => setPref("gap", v)}
        />
      </div>
    {/if}
    <div class="field">
      {t("reader.background")}
      <div class="swatches">
        {#each Object.entries(PAGE_COLORS) as [id, c] (id)}
          <button
            class="sw"
            class:on={mangaPrefs.bg === id}
            style="background: {c.bg}"
            onclick={() => setPref("bg", id as PageColor)}
            title={t(`reader.bg.${id as PageColor}`)}
            aria-label={t(`reader.bg.${id as PageColor}`)}
          ></button>
        {/each}
      </div>
    </div>
    <Switch checked={mangaPrefs.autoNext} onchange={(v) => setPref("autoNext", v)}>{t("reader.autoNext")}</Switch>
    <div class="keys faint">{t("reader.keys")}</div>
  </Drawer>
{/if}

{#if picker && chapters}
  <Drawer title={t("reader.chapters")} media onclose={() => (picker = false)}>
    <div class="pick-list">
      {#each chapters as c (c.id)}
        <button
          class="pick-ch"
          class:on={c.id === currentChapter}
          class:done={c.done}
          onclick={() => {
            picker = false;
            onpick?.(c.id);
          }}
          use:scrollIfCurrent={c.id === currentChapter}
        >
          <span class="ellipsis">{c.label}</span>
          {#if c.sub}<span class="sub ellipsis">{c.sub}</span>{/if}
        </button>
      {/each}
    </div>
  </Drawer>
{/if}

<style>
  .reader {
    position: fixed;
    inset: 0;
    z-index: 20;
    background: var(--page-bg);
    color: var(--page-fg);
    overflow: hidden;
    user-select: none;
    -webkit-user-select: none;
  }
  /* ---------- cascada ---------- */
  .scroller {
    position: absolute;
    inset: 0;
    overflow-y: auto;
    overflow-x: hidden;
    overscroll-behavior: contain;
    /* Barra de scroll fina con el color de la pagina (se ve sobre negro y sobre blanco). */
    --sb-thumb: color-mix(in srgb, var(--page-fg) 26%, transparent);
    --sb-hover: color-mix(in srgb, var(--page-fg) 48%, transparent);
    --sb-active: color-mix(in srgb, var(--accent) 80%, transparent);
  }
  .column {
    display: flex;
    flex-direction: column;
    gap: var(--gap);
    width: min(100%, var(--maxw));
    margin: 0 auto;
  }
  .fit-height .column {
    width: auto;
    align-items: center;
  }
  .pg {
    position: relative;
    width: 100%;
    background: color-mix(in srgb, var(--page-fg) 4%, transparent);
    overflow: hidden;
  }
  .fit-height .pg {
    height: 100dvh;
    width: auto;
  }
  .fit-original .column {
    width: auto;
    align-items: center;
  }
  .pg img,
  .pg canvas {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    display: block;
    animation: pgin 0.3s ease;
  }
  @keyframes pgin {
    from {
      opacity: 0;
    }
  }
  .pg-n {
    position: absolute;
    left: 50%;
    top: 50%;
    transform: translate(-50%, -50%);
    font-family: var(--mono);
    font-size: 22px;
    font-weight: 600;
    opacity: 0.25;
    z-index: -1;
  }
  .pg img ~ .pg-n {
    display: none;
  }
  .pg-err {
    position: absolute;
    inset: 0;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 8px;
    color: var(--page-fg);
    opacity: 0.8;
    font-weight: 600;
  }
  .divider {
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 48px 16px;
    font-weight: 650;
    font-size: 15px;
    opacity: 0.75;
  }
  .divider span {
    padding: 8px 16px;
    border-radius: 999px;
    border: 1px solid color-mix(in srgb, var(--page-fg) 25%, transparent);
  }
  .end {
    min-height: 60dvh;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 14px;
    padding: 40px 16px calc(80px + env(safe-area-inset-bottom));
    text-align: center;
    font-size: 15px;
  }
  .end p {
    margin: 0;
    opacity: 0.8;
  }
  /* ---------- paginado ---------- */
  .strip-host {
    position: absolute;
    inset: 0;
    overflow: hidden;
    touch-action: pan-y pinch-zoom;
  }
  .strip {
    display: flex;
    height: 100%;
    transition: transform 0.32s cubic-bezier(0.2, 0.8, 0.2, 1);
    will-change: transform;
  }
  .strip.rtl {
    flex-direction: row-reverse;
    justify-content: flex-start;
  }
  /* En RTL las pantallas van de derecha a izquierda: la tira se ancla a la derecha. */
  .strip.rtl {
    position: absolute;
    right: 0;
  }
  .strip.dragging {
    transition: none;
  }
  .slide {
    flex: none;
    width: 100vw;
    height: 100%;
    display: flex;
    /* safe: si la pagina es mas alta que la pantalla, arranca arriba (centrada se cortaria). */
    align-items: safe center;
    justify-content: center;
    overflow-y: auto;
    overflow-x: hidden;
  }
  .sp {
    height: 100%;
    width: 100%;
    display: flex;
    align-items: center;
    justify-content: center;
    position: relative;
  }
  .sp.half {
    width: 50%;
  }
  .double .sp:first-child {
    justify-content: flex-end;
  }
  .double .sp:last-child {
    justify-content: flex-start;
  }
  .sp img {
    max-width: 100%;
    max-height: 100%;
    object-fit: contain;
    display: block;
    animation: pgin 0.25s ease;
  }
  /* Ajustar al ancho: la pagina ocupa el ancho y se scrollea para abajo si es mas alta. */
  .reader:not(.fit-height):not(.fit-original) .slide:not(.double) .sp {
    align-items: flex-start;
    height: auto;
    min-height: 100%;
  }
  .reader:not(.fit-height):not(.fit-original) .slide:not(.double) .sp img {
    width: min(100%, var(--maxw));
    max-height: none;
    margin: auto;
  }
  .fit-original .sp img {
    max-width: none;
    max-height: none;
  }
  /* PDF en paginado: al ancho (scrollea si es mas alta) o al alto (entra entera). */
  .canvas-host {
    position: relative;
    width: min(100%, var(--maxw));
    margin: auto;
    background: #fff;
  }
  .fit-height .canvas-host,
  .double .canvas-host {
    width: auto;
    height: 100%;
    max-width: 100%;
  }
  .canvas-host canvas {
    width: 100%;
    height: 100%;
    display: block;
  }
  .sp :global(.faint-ico) {
    opacity: 0.4;
  }
  .slide.end {
    flex-direction: column;
    gap: 14px;
    text-align: center;
    font-size: 15px;
  }
  .slide.end p {
    margin: 0;
    opacity: 0.8;
  }
  /* ---------- barras ---------- */
  /* El lector va sin zoom (la pagina usa la pantalla de verdad); lo que es interfaz encima de la
     pagina se agranda con "Tamaño de la interfaz" (lib/scale.svelte.ts). */
  .bar,
  .side,
  .end > *,
  .divider span,
  .pg-err {
    zoom: var(--ui-zoom, 1);
  }
  .bar {
    position: absolute;
    left: 0;
    right: 0;
    z-index: 5;
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 8px 10px;
    color: var(--on-media);
    background: color-mix(in srgb, #0b0b0d 82%, transparent);
    backdrop-filter: blur(16px) saturate(140%);
  }
  .bar.top {
    top: 0;
    padding-top: calc(8px + env(safe-area-inset-top));
    border-bottom: 1px solid var(--media-line);
  }
  .bar.bottom {
    bottom: 0;
    padding-bottom: calc(10px + env(safe-area-inset-bottom));
    border-top: 1px solid var(--media-line);
  }
  .bar .icon-btn {
    width: 40px;
    height: 40px;
    color: var(--on-media);
    flex: none;
  }
  .bar .icon-btn:hover {
    background: rgba(255, 255, 255, 0.1);
  }
  .bar .icon-btn:disabled {
    opacity: 0.3;
  }
  .titles {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    line-height: 1.25;
  }
  .t1 {
    font-weight: 650;
    font-size: 14px;
  }
  .titles .link {
    align-self: flex-start;
    max-width: 100%;
    text-align: left;
    border-radius: 4px;
    text-decoration: underline;
    text-decoration-color: transparent;
    text-underline-offset: 3px;
    transition: text-decoration-color 0.15s, color 0.15s;
  }
  .titles .link:hover {
    text-decoration-color: currentColor;
  }
  .t2.link {
    display: inline-flex;
    align-items: center;
    gap: 3px;
  }
  .t2 {
    font-size: 12px;
    color: var(--on-media-2);
  }
  .scrub {
    flex: 1;
    display: flex;
    align-items: center;
    padding: 0 6px;
  }
  .scrub.rtl {
    transform: scaleX(-1);
  }
  .scrub input {
    width: 100%;
    height: 28px;
    padding: 0;
    border: 0;
    background: none;
    accent-color: var(--accent);
    cursor: pointer;
  }
  .count {
    min-width: 64px;
    text-align: center;
    font-size: 12.5px;
    font-variant-numeric: tabular-nums;
    color: var(--on-media-2);
  }
  .side {
    position: absolute;
    top: 50%;
    z-index: 4;
    width: 46px;
    height: 90px;
    margin-top: -45px;
    display: grid;
    place-items: center;
    color: var(--on-media);
    background: color-mix(in srgb, #0b0b0d 55%, transparent);
    border-radius: 12px;
    opacity: 0.8;
  }
  .side:hover {
    opacity: 1;
  }
  .side.l {
    left: 10px;
  }
  .side.r {
    right: 10px;
  }
  /* ---------- ajustes ---------- */
  .swatches {
    display: flex;
    gap: 10px;
  }
  .sw {
    width: 36px;
    height: 36px;
    border-radius: 50%;
    border: 2px solid var(--media-line);
    transition: transform 0.15s cubic-bezier(0.34, 1.56, 0.64, 1);
  }
  .sw:hover {
    transform: scale(1.08);
  }
  .sw.on {
    border-color: var(--accent);
    box-shadow: 0 0 0 3px color-mix(in srgb, var(--accent) 35%, transparent);
  }
  .keys {
    font-size: 11.5px;
    line-height: 1.6;
  }
  .pick-list {
    display: flex;
    flex-direction: column;
    gap: 2px;
    margin: -6px -8px;
  }
  .pick-ch {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 1px;
    padding: 9px 12px;
    border-radius: var(--radius-sm);
    text-align: left;
    content-visibility: auto;
    contain-intrinsic-size: auto 50px;
  }
  .pick-ch:hover {
    background: var(--hover);
  }
  .pick-ch.on {
    background: color-mix(in srgb, var(--accent) 18%, transparent);
    box-shadow: inset 3px 0 0 var(--accent);
  }
  .pick-ch.done {
    opacity: 0.55;
  }
  .pick-ch .sub {
    font-size: 11.5px;
    color: var(--text-3);
  }
  @media (max-width: 759px) {
    .hide-phone,
    .side {
      display: none;
    }
  }
  @media (hover: none) {
    .side {
      display: none;
    }
  }
</style>
