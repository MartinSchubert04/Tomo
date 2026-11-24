// Atajos de teclado: la lista que muestra Ajustes → Atajos (los de verdad estan en App.svelte, la
// ficha y el lector; si se agrega uno, va tambien aca).
import type { t } from "./i18n.svelte";

type Key = Parameters<typeof t>[0];

export type Shortcut = { keys: string[]; label: Key };
export type ShortcutGroup = { title: Key; items: Shortcut[] };

export const SHORTCUTS: ShortcutGroup[] = [
  {
    title: "keys.general",
    items: [
      { keys: ["/", "Ctrl K"], label: "keys.search" },
      { keys: ["G H"], label: "keys.goHome" },
      { keys: ["G E"], label: "keys.goExplore" },
      { keys: ["G L"], label: "keys.goLibrary" },
      { keys: ["G B"], label: "keys.goBooks" },
      { keys: ["G R"], label: "keys.goHistory" },
      { keys: ["G S"], label: "keys.goSettings" },
      { keys: ["Esc"], label: "keys.close" },
    ],
  },
  {
    title: "keys.detail",
    items: [{ keys: ["R"], label: "keys.read" }],
  },
  {
    title: "keys.reader",
    items: [
      { keys: ["←", "→", "A", "D"], label: "keys.turn" },
      { keys: ["↓", "↑", "Espacio", "AvPag"], label: "keys.scroll" },
      { keys: ["Inicio", "Fin"], label: "keys.firstLast" },
      { keys: ["[", "]"], label: "keys.chapter" },
      { keys: ["M"], label: "keys.mode" },
      { keys: ["F"], label: "keys.fullscreen" },
      { keys: ["Esc"], label: "keys.back" },
    ],
  },
];

/** Atajos de dos teclas ("G" y despues "H"): a que ruta va cada segunda tecla. */
export const GO_KEYS: Record<string, string> = { h: "/", e: "/explore", l: "/library", b: "/books", r: "/history", s: "/settings" };
