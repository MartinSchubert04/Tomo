// Catalogos de libros gratis (docs/API.md, "Catalogs"): la lista (una vez por sesion), una
// entrada como tarjeta de portada y el nombre de un idioma en el idioma de la interfaz.
import { api } from "./api";
import { i18n } from "./i18n.svelte";
import type { BookFormat, Catalog, CatalogEntry, WorkCard } from "./types";

let cached: Promise<Catalog[]> | null = null;

/** Los catalogos prendidos (los de español primero, como los manda el server). */
export function loadCatalogs(fresh = false): Promise<Catalog[]> {
  if (!cached || fresh) cached = api.catalogs().catch((e) => ((cached = null), Promise.reject(e)));
  return cached;
}

const FORMATS = new Set(["epub", "pdf", "cbz", "mobi", "azw3", "fb2", "txt"]);

/** Una entrada de catalogo con la forma de una tarjeta (CoverCard). El id no es un libro: la
 *  tarjeta abre el detalle con `onopen`. */
export function entryWork(e: CatalogEntry): WorkCard {
  const f = e.formats.find((x) => FORMATS.has(x)) as BookFormat | undefined;
  return { id: `cat-${e.id.slice(0, 40)}`, kind: "book", title: e.title, cover: e.cover, format: f };
}

const names = new Map<string, Intl.DisplayNames>();
/** "es" → "español" (o "Spanish" con la interfaz en ingles). */
export function langName(code: string | null | undefined): string {
  if (!code) return "";
  try {
    let d = names.get(i18n.lang);
    if (!d) names.set(i18n.lang, (d = new Intl.DisplayNames([i18n.lang], { type: "language" })));
    const n = d.of(code) ?? code;
    return n.charAt(0).toUpperCase() + n.slice(1);
  } catch {
    return code;
  }
}
