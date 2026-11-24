// Formatos chicos que se usan en varias vistas.
import { num, t } from "./i18n.svelte";
import type { Chapter, Progress } from "./types";

export function bytes(n: number): string {
  if (n < 1024) return `${n} B`;
  const u = ["KB", "MB", "GB"];
  let v = n / 1024;
  let i = 0;
  while (v >= 1024 && i < u.length - 1) {
    v /= 1024;
    i++;
  }
  return `${num(v, { maximumFractionDigits: v < 10 ? 1 : 0 })} ${u[i]}`;
}

/** "Cap. 12" si se sabe el numero; si no, el nombre que da la fuente. */
export function chapterShort(c: Pick<Chapter, "number" | "name">): string {
  return c.number >= 0 ? t("reader.chapterN", { n: num(c.number, { maximumFractionDigits: 2, useGrouping: false }) }) : c.name;
}

/** El titulo propio del capitulo, sin el "Capitulo 12" que ya dice chapterShort ("Capitulo 12:
 *  El regreso" → "El regreso"). Vacio si el nombre es solo el numero. */
export function chapterTitle(c: Pick<Chapter, "number" | "name">): string {
  const name = c.name.trim();
  if (c.number < 0) return "";
  const m = /^(?:cap[ií]tulo|chapter|cap\.?|ch\.?|episodio|episode|ep\.?)\s*[\d.,]+\s*[:\-–—.]?\s*(.*)$/i.exec(name);
  if (m) return m[1].trim();
  return /^[\d.,]+$/.test(name) ? "" : name;
}

/** Donde quedo una lectura: "Cap. 12 · pág. 8/40" o "45 %". */
export function progressLabel(p: Progress, chapterName: string | null, kind: "manga" | "book"): string {
  if (kind === "book") return `${Math.round(p.fraction * 100)} %`;
  const page = Number(p.position);
  const label = chapterName ?? "";
  return isFinite(page) && !p.done ? `${label} · ${t("reader.pageShort", { n: page + 1 })}` : label;
}

/** Orden natural ("cap 2" antes que "cap 10"). */
export const natural = new Intl.Collator(undefined, { numeric: true, sensitivity: "base" });
