// Capitulos agrupados por numero. Las fuentes multi-idioma (MangaDex "ALL") y los mangas con
// varios scans traen el mismo numero varias veces ("Cap. 6 [en] RESET SCANS", "Cap. 6 [es-419]
// Por amor al arte"): la lista muestra una fila por numero y cada fila elige su version por
// defecto con esta regla (la misma en la ficha, el lector y "Seguir en el cap."):
//  1. el idioma y el scan que se estan leyendo, si ese numero los tiene (siguiente/anterior);
//  2. los idiomas del usuario en su orden (Ajustes → Lectura, "Idioma de los capitulos");
//  3. el scan elegido para ese manga ("Preferir scan", o el ultimo que se leyo);
//  4. la mas nueva.
// Un numero cuenta como leido si cualquiera de sus versiones lo esta (igual que en el server).
import { session } from "./session.svelte";
import { read, write } from "./store";
import type { Chapter, Progress } from "./types";

export type ChapterGroup = {
  /** "n12" o, sin numero, "c<id>" (cada uno es su propio grupo). */
  key: string;
  number: number;
  /** Las versiones, en el orden de la fuente (mas nueva primero). */
  variants: Chapter[];
};

/** Los idiomas de capitulo por defecto: latino, español, ingles. */
export const DEFAULT_CHAPTER_LANGS = ["es-419", "es", "en"];

export function chapterLangs(): string[] {
  return session.prefs.chapterLangs ?? DEFAULT_CHAPTER_LANGS;
}

/** Agrupa por numero manteniendo el orden de la lista (del mas nuevo al mas viejo). */
export function groupChapters(chs: Chapter[]): ChapterGroup[] {
  const out: ChapterGroup[] = [];
  const byKey = new Map<string, ChapterGroup>();
  for (const c of chs) {
    const key = c.number >= 0 ? `n${c.number}` : `c${c.id}`;
    let g = byKey.get(key);
    if (!g) {
      g = { key, number: c.number, variants: [] };
      byKey.set(key, g);
      out.push(g);
    }
    g.variants.push(c);
  }
  return out;
}

// ---------- scan preferido por manga (en este dispositivo) ----------
const scanKey = (work: string) => `library.scan.${work}`;
export function preferredScan(work: string): string | null {
  return read(scanKey(work));
}
export function setPreferredScan(work: string, scan: string | null) {
  write(scanKey(work), scan || null);
}

export type PickCtx = {
  /** El idioma y scan que se estan leyendo (para seguir igual en el siguiente). */
  lang?: string | null;
  scan?: string | null;
  /** El scan preferido del manga. */
  preferScan?: string | null;
};

/** La version por defecto de un numero (la regla de arriba). */
export function pickVariant(g: ChapterGroup, ctx: PickCtx = {}): Chapter {
  if (g.variants.length === 1) return g.variants[0];
  const langs = chapterLangs();
  const langRank = (c: Chapter) => {
    if (ctx.lang !== undefined && c.lang === ctx.lang && ctx.lang !== null) return -1;
    if (c.lang === null) return langs.length; // el idioma de la fuente, antes que los no elegidos
    const i = langs.indexOf(c.lang);
    return i >= 0 ? i : langs.length + 1;
  };
  const scanRank = (c: Chapter) => (ctx.scan && c.scanlator === ctx.scan ? 0 : ctx.preferScan && c.scanlator === ctx.preferScan ? 1 : 2);
  return [...g.variants].sort((a, b) => langRank(a) - langRank(b) || scanRank(a) - scanRank(b) || b.date - a.date)[0];
}

export function groupDone(g: ChapterGroup, progress: Record<string, Progress>): boolean {
  return g.variants.some((v) => progress[v.id]?.done);
}

/** El progreso a medias de un numero (la version que se estaba leyendo), si hay. */
export function groupProgress(g: ChapterGroup, progress: Record<string, Progress>): Progress | null {
  let best: Progress | null = null;
  for (const v of g.variants) {
    const p = progress[v.id];
    if (p && (!best || p.updatedAt > best.updatedAt)) best = p;
  }
  return best;
}

export function groupOf(groups: ChapterGroup[], chapterId: string): number {
  return groups.findIndex((g) => g.variants.some((v) => v.id === chapterId));
}

/** "LAT", "ES", "EN"... para la pastilla del idioma. */
export function langBadge(lang: string | null): string {
  if (!lang) return "";
  if (lang === "es-419") return "LAT";
  return lang.split("-")[0].toUpperCase();
}
