// Ajustes de los lectores, de cada dispositivo (en el celular se lee en cascada, en la PC capaz
// paginado): quedan en localStorage, no viajan al server.
import { readJson, write } from "./store";

export type ReadMode = "cascade" | "paged" | "double";
export type ReadDir = "rtl" | "ltr";
export type Fit = "width" | "height" | "original";
export type PageColor = "black" | "gray" | "white" | "sepia";

export type MangaPrefs = {
  mode: ReadMode;
  dir: ReadDir;
  fit: Fit;
  /** Ancho maximo de la cascada en px. */
  maxWidth: number;
  /** Espacio entre paginas en cascada. */
  gap: 0 | 8 | 24;
  bg: PageColor;
  /** Pasar al capitulo siguiente solo al terminar (en cascada, se agrega abajo). */
  autoNext: boolean;
};

export type BookFont = "original" | "literata" | "inter";
export type BookPrefs = {
  font: BookFont;
  size: number; // %
  lineHeight: number;
  margin: number; // px a cada lado
  justify: boolean;
  flow: "paginated" | "scrolled";
  color: PageColor;
};

const MKEY = "library.reader.manga";
const BKEY = "library.reader.book";

const phone = typeof matchMedia !== "undefined" && matchMedia("(max-width: 759px)").matches;

export const mangaPrefs = $state<MangaPrefs>(
  readJson<MangaPrefs>(MKEY, {
    mode: "cascade",
    dir: "rtl",
    fit: "width",
    maxWidth: 900,
    gap: 0,
    bg: "black",
    autoNext: true,
  }),
);

export const bookPrefs = $state<BookPrefs>(
  readJson<BookPrefs>(BKEY, {
    font: "literata",
    size: 100,
    lineHeight: 1.6,
    margin: phone ? 18 : 48,
    justify: true,
    flow: "paginated",
    color: "black",
  }),
);

export function saveMangaPrefs() {
  write(MKEY, JSON.stringify($state.snapshot(mangaPrefs)));
}
export function saveBookPrefs() {
  write(BKEY, JSON.stringify($state.snapshot(bookPrefs)));
}

/** Colores de pagina (no dependen del tema: el lector es su propio mundo). */
export const PAGE_COLORS: Record<PageColor, { bg: string; fg: string }> = {
  black: { bg: "#000000", fg: "#d6d6d6" },
  gray: { bg: "#1e1f22", fg: "#d8d8d8" },
  white: { bg: "#ffffff", fg: "#1a1a1a" },
  sepia: { bg: "#f4ecd8", fg: "#3b2f20" },
};
