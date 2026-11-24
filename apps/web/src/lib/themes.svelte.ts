// Temas y paletas (portados de Ethereon y Mythos). Los colores base estan en src/styles/themes.css y
// las paletas en scripts/palettes.py (que genera palettes.css y palettes.gen.ts).
//
// Son de cada usuario: viajan en sus preferencias del server (`prefs.theme`, `prefs.palettes`) y
// se aplican al entrar (lib/session.svelte.ts). En localStorage queda una copia del ultimo
// usuario de este dispositivo: la usa el script de index.html para pintar el tema antes de montar.
import { PALETTES } from "./palettes.gen";
import { read, write } from "./store";

export const THEMES = [
  { id: "default", name: "Default" },
  { id: "warp", name: "Warp" },
  { id: "poolsuite", name: "Poolsuite FM" },
  { id: "haze", name: "Haze" },
  { id: "athanor", name: "Athanor" },
  { id: "neovim", name: "Neovim" },
] as const;

export type ThemeId = (typeof THEMES)[number]["id"];

export function palettesOf(t: ThemeId) {
  return PALETTES[t] ?? [];
}

// Mismas claves que el script de index.html.
const KEY = "library.theme";
const PKEY = (t: ThemeId) => `library.palette.${t}`;

export function isTheme(t: unknown): t is ThemeId {
  return THEMES.some((x) => x.id === t);
}

const isPalette = (t: ThemeId, p: unknown): p is string => typeof p === "string" && palettesOf(t).some((x) => x.id === p);

export const DEFAULT_THEME: ThemeId = "default";

/** El tema en uso (reactivo). */
export const look = $state<{ theme: ThemeId; palettes: Partial<Record<ThemeId, string>> }>({
  theme: ((t) => (isTheme(t) ? t : DEFAULT_THEME))(read(KEY)),
  palettes: Object.fromEntries(THEMES.map((t) => [t.id, read(PKEY(t.id)) ?? ""])),
});

/** La paleta elegida para un tema (cada tema recuerda la suya). */
export function currentPalette(t: ThemeId): string {
  const p = look.palettes[t];
  return isPalette(t, p) ? p : (palettesOf(t)[0]?.id ?? "");
}

export function allPalettes(): Record<string, string> {
  return Object.fromEntries(THEMES.map((t) => [t.id, currentPalette(t.id)]));
}

export function applyTheme(t: ThemeId, palette = currentPalette(t)) {
  const el = document.documentElement;
  look.theme = t;
  look.palettes[t] = palette;
  el.dataset.theme = t;
  el.dataset.palette = palette;
  write(KEY, t);
  write(PKEY(t), palette);
  // La barra del navegador del celular con el fondo del tema.
  requestAnimationFrame(() => {
    const bg = getComputedStyle(el).getPropertyValue("--bg").trim();
    const meta = document.querySelector('meta[name="theme-color"]');
    if (bg && meta) meta.setAttribute("content", bg);
  });
}

/** El tema guardado de un usuario. Devuelve si habia. */
export function applyRemoteTheme(theme: unknown, palettes: unknown): boolean {
  if (!isTheme(theme)) return false;
  const got = palettes && typeof palettes === "object" ? (palettes as Record<string, unknown>) : {};
  for (const t of THEMES) {
    const p = got[t.id];
    look.palettes[t.id] = isPalette(t.id, p) ? p : (palettesOf(t.id)[0]?.id ?? "");
    write(PKEY(t.id), look.palettes[t.id]!);
  }
  applyTheme(theme);
  return true;
}
