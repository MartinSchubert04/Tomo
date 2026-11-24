// Rutas con hash (#/manga/s123): el server sirve index.html para todo y cada vista se carga
// recien cuando hace falta (import dinamico). Portado de Mythos.
import type { Component } from "svelte";
import { tick } from "svelte";
import { reduceMotion } from "./motion";
import { markDown } from "./net.svelte";

export type ViewName =
  | "home"
  | "explore"
  | "search"
  | "library"
  | "books"
  | "history"
  | "manga"
  | "reader"
  | "book"
  | "settings"
  | "admin";

// eslint-disable-next-line @typescript-eslint/no-explicit-any
type Loader = () => Promise<{ default: Component<any> }>;
const loaders: Record<ViewName, Loader> = {
  home: () => import("../views/Home.svelte"),
  explore: () => import("../views/Explore.svelte"),
  search: () => import("../views/Search.svelte"),
  library: () => import("../views/Library.svelte"),
  books: () => import("../views/Books.svelte"),
  history: () => import("../views/History.svelte"),
  manga: () => import("../views/Manga.svelte"),
  reader: () => import("../views/Reader.svelte"),
  book: () => import("../views/BookReader.svelte"),
  settings: () => import("../views/Settings.svelte"),
  admin: () => import("../views/Admin.svelte"),
};

/** Las vistas que ocupan toda la pantalla (sin barra lateral ni pestañas). */
export const FULLSCREEN: ViewName[] = ["reader", "book"];

// eslint-disable-next-line @typescript-eslint/no-explicit-any
const loaded = $state<Partial<Record<ViewName, Component<any>>>>({});
const pending = new Map<ViewName, Promise<void>>();

/** Carga el codigo de una vista (se puede llamar antes, por ejemplo al pasar el mouse). */
export function loadView(v: ViewName): Promise<void> {
  if (loaded[v]) return Promise.resolve();
  let p = pending.get(v);
  if (!p) {
    p = loaders[v]().then((m) => {
      loaded[v] = m.default;
    });
    p.catch(() => {
      pending.delete(v);
      markDown();
    });
    pending.set(v, p);
  }
  return p;
}

export function viewComponent(v: ViewName) {
  return loaded[v];
}

type Route = { path: string; view: ViewName; params: string[]; query: URLSearchParams; key: string; pop: boolean };

function parse(hash: string, pop = false): Route {
  const raw = hash.replace(/^#/, "") || "/";
  const [path, qs = ""] = raw.split("?");
  const parts = path.split("/").filter(Boolean).map(decodeURIComponent);
  const query = new URLSearchParams(qs);
  let view: ViewName = "home";
  let params: string[] = [];
  switch (parts[0]) {
    case "explore":
      view = "explore";
      params = parts.slice(1, 2);
      break;
    case "manga":
      view = "manga";
      params = parts.slice(1, 2);
      break;
    case "read":
      view = "reader";
      params = parts.slice(1, 3);
      break;
    case "book":
      view = "book";
      params = parts.slice(1, 2);
      break;
    case "search":
    case "library":
    case "books":
    case "history":
    case "settings":
    case "admin":
      view = parts[0];
      break;
  }
  // La busqueda y explorar no se vuelven a montar al cambiar la query; el lector tampoco al
  // pasar de capitulo (cambia la ruta, pero es el mismo lector).
  const key = view === "search" || view === "explore" ? view : view === "reader" ? `reader/${params[0]}` : path;
  return { path, view, params, query, key, pop };
}

export const route = $state<Route>(parse(location.hash));

function apply(r: Route) {
  Object.assign(route, r);
}

let popped = "";

function sync() {
  const r = parse(location.hash, location.hash === popped);
  if (r.path === route.path && r.query.toString() === route.query.toString()) return;
  loadView(r.view).finally(() => apply(r));
}
window.addEventListener("popstate", () => {
  popped = location.hash;
  sync();
});
window.addEventListener("hashchange", sync);

if (import.meta.hot) import.meta.hot.accept(() => location.reload());

/** Ir a una ruta. Con `vt`, View Transitions (la portada vuela de la tarjeta al detalle). */
export async function go(path: string, o: { replace?: boolean; vt?: boolean } = {}) {
  const url = "#" + path;
  popped = "";
  if (o.replace) history.replaceState(history.state, "", url);
  else if (location.hash !== url) history.pushState(null, "", url);
  const r = parse(url);
  await loadView(r.view).catch(() => {});
  if (!o.vt || !document.startViewTransition || reduceMotion()) {
    apply(r);
    return;
  }
  document.startViewTransition(async () => {
    apply(r);
    await tick();
  });
}

/** Cambia la query sin agregar una entrada al historial (la busqueda en vivo). */
export function setQuery(params: Record<string, string>) {
  const clean = Object.fromEntries(Object.entries(params).filter(([, v]) => v !== ""));
  const q = new URLSearchParams(clean).toString();
  history.replaceState(history.state, "", "#" + route.path + (q ? "?" + q : ""));
  route.query = new URLSearchParams(q);
}

export function back(fallback = "/") {
  if (history.length > 1 && history.state !== "root") history.back();
  else go(fallback, { replace: true });
}

// La primera entrada queda marcada: "atras" desde ahi va al inicio en vez de salir de la app.
if (!history.state) history.replaceState("root", "", location.href);
