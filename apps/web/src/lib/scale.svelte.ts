// Tamaño de la interfaz (Ajustes → Apariencia). En un monitor 2K o 4K a escala 100 % todo
// quedaba chico: se agranda con `zoom` en <html>. Es de cada dispositivo (localStorage), no viaja
// con el usuario: en el celu y en la PC se quieren tamaños distintos.
//
// Cómo se mide con zoom (Chrome 128+, zoom estandarizado; probado en Chromium 151):
// - getBoundingClientRect, clientX/clientY, innerWidth/innerHeight: en px de la ventana (ya
//   agrandados). Los largos que se escriben en style (left, top, width) se multiplican por el zoom:
//   para poner algo donde marca el mouse hay que dividir por `zf()` (ver `toCss`).
// - scrollTop, offsetWidth, clientHeight: en px CSS (sin agrandar).
// - 100vh / 100dvh / 100vw se multiplican: en CSS se usa `--vh` / `--vw` (app.css), que ya
//   vienen divididos por `--zoom`.
// - Las media queries miden la ventana real (2560 sigue siendo 2560); las container queries
//   miden en px CSS (2560 / 1.33 = 1925). Para layouts que dependen del ancho útil, container
//   queries sobre `.content`.
//
// Los lectores van sin zoom (la página usa la pantalla de verdad y foliate-js hace muchas
// cuentas con rects): solo sus barras y hojas de ajustes se agrandan, con `zoom: var(--ui-zoom)`.
import { read, write } from "./store";

export const UI_SCALES = ["auto", 90, 100, 110, 125, 140, 160] as const;
export type UiScale = (typeof UI_SCALES)[number];

const KEY = "library.uiScale";

function parse(v: string | null): UiScale {
  const n = Number(v);
  return (UI_SCALES as readonly (string | number)[]).includes(n) ? (n as UiScale) : "auto";
}

/** "Auto": 1 hasta 1800 px de ancho, 1.15 desde ahí, 1.33 desde 2300. En el celu siempre 1.
 *  (Mismas cuentas que el script de index.html, que lo aplica antes de pintar.) */
export function autoFactor(): number {
  if (matchMedia("(max-width: 759px)").matches) return 1;
  const w = innerWidth;
  return w >= 2300 ? 1.33 : w >= 1800 ? 1.15 : 1;
}

export const scale = $state<{ pref: UiScale; factor: number }>({ pref: parse(read(KEY)), factor: 1 });

// El zoom aplicado ahora en <html> (1 en los lectores). Variable común: se lee en cada evento
// del mouse y no hace falta que sea reactiva.
let applied = 1;
let fullscreen = false;

/** El zoom de <html> ahora mismo. */
export const zf = () => applied;

/** De px de la ventana (rects, clientX) a px CSS para escribir en style. */
export const toCss = (px: number) => px / applied;

export function applyScale(full = fullscreen) {
  fullscreen = full;
  const factor = scale.pref === "auto" ? autoFactor() : scale.pref / 100;
  scale.factor = factor;
  applied = full ? 1 : factor;
  const el = document.documentElement;
  el.style.zoom = applied === 1 ? "" : String(applied);
  el.style.setProperty("--zoom", String(applied));
  el.style.setProperty("--ui-zoom", String(factor));
}

export function setScale(p: UiScale) {
  scale.pref = p;
  write(KEY, p === "auto" ? null : String(p));
  applyScale();
}

let installed = false;
/** Recalcula al cambiar el tamaño de la ventana ("Auto" depende del ancho). */
export function installScale() {
  if (installed) return;
  installed = true;
  let raf = 0;
  addEventListener("resize", () => {
    if (scale.pref !== "auto" || raf) return;
    raf = requestAnimationFrame(() => {
      raf = 0;
      applyScale();
    });
  });
}
