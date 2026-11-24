// Las fuentes instaladas (se piden una vez y se reusan entre vistas) y las fijadas por el usuario.
import { api } from "./api";
import { session, setPrefs } from "./session.svelte";
import type { Source } from "./types";

export const sources = $state<{ list: Source[] | null; error: string }>({ list: null, error: "" });

let loading: Promise<Source[]> | null = null;

export function loadSources(force = false): Promise<Source[]> {
  if (sources.list && !force) return Promise.resolve(sources.list);
  if (!loading || force) {
    loading = api
      .sources()
      .then((l) => {
        sources.list = l;
        sources.error = "";
        return l;
      })
      .catch((e) => {
        loading = null;
        sources.error = String(e?.message ?? e);
        throw e;
      });
  }
  return loading;
}

/** Los idiomas de fuente que se ven si el usuario no eligio: español, latino y multi-idioma. */
export const DEFAULT_LANGS = ["es", "es-419", "all"];

/** Los idiomas de fuente elegidos (Ajustes → Fuentes). */
export function sourceLangs(): string[] {
  return session.prefs.sourceLangs ?? DEFAULT_LANGS;
}

export function setSourceLangs(langs: string[]) {
  setPrefs({ sourceLangs: langs });
}

const SPANISH = DEFAULT_LANGS;

/** Las fuentes para mostrar: las fijadas primero (en su orden), despues las en español, despues
 *  el resto. Sin NSFW salvo que el usuario las pida. */
export function orderedSources(list: Source[]): Source[] {
  const pinned = session.prefs.sources ?? [];
  const nsfw = !!session.prefs.nsfw;
  const ok = list.filter((s) => nsfw || s.nsfw !== "nsfw");
  const rank = (s: Source) => {
    const p = pinned.indexOf(s.id);
    if (p >= 0) return p;
    return 1000 + (SPANISH.includes(s.lang) ? 0 : 1000);
  };
  return [...ok].sort((a, b) => rank(a) - rank(b) || a.name.localeCompare(b.name));
}

/** Las fuentes para elegir: las de los idiomas del usuario y las fijadas (aunque sean de otro
 *  idioma). Con `all`, todas. MangaDex sola trae ~40 variantes (una por idioma): sin este filtro
 *  Explorar era una lista de "MangaDex (AF)", "MangaDex (AR)"... */
export function visibleSources(list: Source[], all = false): Source[] {
  const langs = sourceLangs();
  const ordered = orderedSources(list);
  if (all || !langs.length) return ordered;
  return ordered.filter((s) => isPinned(s.id) || langs.includes(s.lang));
}

/** La fuente principal del usuario (inicio): la primera fijada, o la primera de sus idiomas. */
export function mainSource(list: Source[]): Source | null {
  return visibleSources(list)[0] ?? orderedSources(list)[0] ?? null;
}

/** Fuentes para la busqueda global: las fijadas o, sin fijadas, las de sus idiomas (12 como
 *  mucho, igual que el server). */
export function searchSources(list: Source[]): string[] {
  const pinned = (session.prefs.sources ?? []).filter((id) => list.some((s) => s.id === id));
  if (pinned.length) return pinned;
  return visibleSources(list)
    .slice(0, 12)
    .map((s) => s.id);
}

/** Los idiomas que tienen las fuentes instaladas, los del usuario primero. */
export function installedLangs(list: Source[]): string[] {
  const mine = sourceLangs();
  const all = [...new Set(list.map((s) => s.lang))];
  return all.sort((a, b) => Number(mine.includes(b)) - Number(mine.includes(a)) || a.localeCompare(b));
}

export function isPinned(id: string) {
  return (session.prefs.sources ?? []).includes(id);
}

export function togglePin(id: string) {
  const cur = session.prefs.sources ?? [];
  setPrefs({ sources: cur.includes(id) ? cur.filter((x) => x !== id) : [...cur, id] });
}

/** El idioma corto para las pastillas ("ES", "ES-419", "ALL"). */
export function langLabel(lang: string): string {
  return lang.toUpperCase();
}
