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

const SPANISH = ["es", "es-419", "all"];

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

/** La fuente principal del usuario (inicio): la primera fijada, o la primera en español. */
export function mainSource(list: Source[]): Source | null {
  return orderedSources(list)[0] ?? null;
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
