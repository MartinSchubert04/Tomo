// La sesion y las preferencias del usuario. Las preferencias son un objeto libre que el server
// guarda tal cual (PUT /api/me/prefs): tema, paletas, idioma, reducir animaciones y las fuentes
// fijadas. Lo que es de cada dispositivo (ajustes del lector) esta en lib/readerPrefs.svelte.ts.
import { api, ApiError, setUnauthorizedHandler } from "./api";
import { isLang, setLang, i18n, type Lang } from "./i18n.svelte";
import { allPalettes, applyRemoteTheme, look, type ThemeId } from "./themes.svelte";
import { read, write } from "./store";
import type { User } from "./types";

export type Prefs = {
  theme?: ThemeId;
  palettes?: Record<string, string>;
  lang?: Lang;
  reduceMotion?: boolean;
  /** Fuentes fijadas (ids), en orden: la busqueda global y el inicio usan estas. */
  sources?: string[];
  /** Mostrar fuentes NSFW en Explorar. */
  nsfw?: boolean;
};

export const session = $state<{ user: User | null; prefs: Prefs; ready: boolean; expired: boolean }>({
  user: null,
  prefs: {},
  ready: false,
  expired: false,
});

const RM_KEY = "library.reduceMotion";

export function applyReduceMotion(on: boolean) {
  const el = document.documentElement;
  if (on) el.setAttribute("data-reduce-motion", "");
  else el.removeAttribute("data-reduce-motion");
  write(RM_KEY, on ? "1" : null);
}

function applyPrefs(p: Prefs) {
  // Sin tema guardado (usuario nuevo): hereda el de este dispositivo y lo guarda.
  if (!applyRemoteTheme(p.theme, p.palettes)) {
    p.theme = look.theme;
    p.palettes = allPalettes();
  }
  if (isLang(p.lang)) setLang(p.lang);
  else p.lang = i18n.lang;
  applyReduceMotion(p.reduceMotion ?? read(RM_KEY) === "1");
}

/** Al abrir: ¿hay sesion? */
export async function restore() {
  try {
    const me = await api.me();
    session.user = me.user;
    session.prefs = (me.prefs ?? {}) as Prefs;
    applyPrefs(session.prefs);
  } catch (e) {
    session.user = null;
    // Server caido al abrir: no es "no hay sesion". Queda el splash y se reintenta (si no, se
    // veria el login y despues de entrar la sesion vieja seguia valida).
    if (!(e instanceof ApiError) || e.status !== 401) {
      setTimeout(restore, 3000);
      return;
    }
  }
  session.ready = true;
}

export async function login(name: string, password: string) {
  const r = await api.login(name, password);
  session.expired = false;
  session.user = r.user;
  try {
    const me = await api.me();
    session.prefs = (me.prefs ?? {}) as Prefs;
  } catch {
    session.prefs = {};
  }
  applyPrefs(session.prefs);
}

export async function logout() {
  try {
    await api.logout();
  } catch {}
  session.user = null;
  session.prefs = {};
  location.hash = "#/";
}

setUnauthorizedHandler(() => {
  if (!session.user) return;
  session.user = null;
  session.expired = true;
});

let saveTimer = 0;
/** Cambia preferencias y las guarda en el server (agrupadas, 600 ms despues del ultimo cambio). */
export function setPrefs(p: Partial<Prefs>) {
  Object.assign(session.prefs, p);
  clearTimeout(saveTimer);
  saveTimer = window.setTimeout(() => {
    api.savePrefs($state.snapshot(session.prefs) as Record<string, unknown>).catch(() => {});
  }, 600);
}
