// Cliente de la API (docs/API.md). La sesion va en la cookie `lib_session`: fetch e <img> la
// mandan solos, asi que aca no hay token. Un 401 avisa a la sesion (vuelve al login con "Tu
// sesion vencio") y un error de red marca el server como caido (lib/net.svelte.ts).
import { has, t } from "./i18n.svelte";
import { markDown, markUp } from "./net.svelte";
import type {
  Book,
  BrowsePage,
  BrowseType,
  Extension,
  Health,
  HistoryEntry,
  LibraryEntry,
  LibraryStatus,
  MangaDetail,
  Me,
  Progress,
  SearchGroup,
  Source,
  User,
  WorkProgress,
} from "./types";

export class ApiError extends Error {
  constructor(
    public status: number,
    public code: string,
    message: string,
  ) {
    super(message);
  }
}

let onUnauthorized: (() => void) | null = null;
export function setUnauthorizedHandler(f: () => void) {
  onUnauthorized = f;
}

async function req<T>(method: string, path: string, body?: unknown, o: { signal?: AbortSignal; raw?: boolean } = {}): Promise<T> {
  let res: Response;
  try {
    res = await fetch("/api" + path, {
      method,
      credentials: "same-origin",
      headers: body !== undefined ? { "Content-Type": "application/json" } : undefined,
      body: body !== undefined ? JSON.stringify(body) : undefined,
      signal: o.signal,
    });
  } catch (e) {
    if ((e as Error).name === "AbortError") throw e;
    markDown();
    throw new ApiError(0, "unreachable", t("common.unreachable"));
  }
  markUp();
  if (!res.ok) {
    let code = "internal";
    let message = "";
    try {
      const j = await res.json();
      code = j.error ?? code;
      message = j.message ?? "";
    } catch {}
    if (res.status === 401 && path !== "/auth/login" && path !== "/me") onUnauthorized?.();
    throw new ApiError(res.status, code, message || t("common.errorStatus", { status: res.status }));
  }
  if (o.raw) return res as unknown as T;
  if (res.status === 204) return undefined as T;
  const ct = res.headers.get("content-type") ?? "";
  if (ct.includes("json")) return (await res.json()) as T;
  const text = await res.text();
  return (text ? text : undefined) as T;
}

/** El texto para mostrar de un error (traducido por codigo si se conoce). */
export function errText(e: unknown): string {
  if (e instanceof ApiError) {
    const k = `srv.${e.code}`;
    return has(k) ? t(k) : e.message;
  }
  if (e instanceof Error) return e.message;
  return String(e);
}

const enc = encodeURIComponent;
function qs(o: Record<string, string | number | undefined | null>) {
  const p = new URLSearchParams();
  for (const [k, v] of Object.entries(o)) if (v !== undefined && v !== null && v !== "") p.set(k, String(v));
  const s = p.toString();
  return s ? "?" + s : "";
}

export const api = {
  health: () => req<Health>("GET", "/health"),
  login: (name: string, password: string) => req<{ user: User }>("POST", "/auth/login", { name, password }),
  logout: () => req<void>("POST", "/auth/logout"),
  me: () => req<Me>("GET", "/me"),
  savePrefs: (p: Record<string, unknown>) => req<void>("PUT", "/me/prefs", p),
  changePassword: (current: string, next: string) => req<void>("POST", "/me/password", { current, next }),

  // admin
  users: () => req<User[]>("GET", "/admin/users"),
  createUser: (name: string, password: string, admin: boolean) => req<User>("POST", "/admin/users", { name, password, admin }),
  updateUser: (id: number, p: { password?: string; admin?: boolean }) => req<void>("PATCH", `/admin/users/${id}`, p),
  deleteUser: (id: number) => req<void>("DELETE", `/admin/users/${id}`),
  extensions: (o: { lang?: string; q?: string; installed?: boolean } = {}) =>
    req<Extension[]>("GET", "/admin/extensions" + qs({ lang: o.lang, q: o.q, installed: o.installed ? 1 : undefined })),
  refreshExtensions: () => req<Extension[]>("POST", "/admin/extensions/refresh"),
  extensionAction: (pkg: string, action: "install" | "uninstall" | "update") =>
    req<Extension>("POST", `/admin/extensions/${enc(pkg)}`, { action }),
  repos: () => req<string[]>("GET", "/admin/repos"),
  setRepos: (r: string[]) => req<void>("PUT", "/admin/repos", r),

  // fuentes
  sources: () => req<Source[]>("GET", "/sources"),
  browse: (source: string, type: BrowseType, page: number, q = "", signal?: AbortSignal) =>
    req<BrowsePage>("GET", `/sources/${enc(source)}/browse` + qs({ type, q, page }), undefined, { signal }),

  // manga
  manga: (id: string, refresh = false) => req<MangaDetail>("GET", `/manga/${enc(id)}` + (refresh ? "?refresh=1" : "")),
  pages: (id: string, chapter: string) => req<{ pages: string[] }>("GET", `/manga/${enc(id)}/chapters/${enc(chapter)}/pages`),

  // biblioteca y progreso
  library: () => req<LibraryEntry[]>("GET", "/library"),
  setLibrary: (id: string, p: { status?: LibraryStatus; favorite?: boolean }) => req<LibraryEntry>("PUT", `/library/${enc(id)}`, p),
  removeLibrary: (id: string) => req<void>("DELETE", `/library/${enc(id)}`),
  progress: (id: string) => req<WorkProgress>("GET", `/progress/${enc(id)}`),
  saveProgress: (id: string, p: Omit<Progress, "updatedAt">) => req<void>("PUT", `/progress/${enc(id)}`, p),
  mark: (id: string, chapters: string[], done: boolean) => req<void>("POST", `/progress/${enc(id)}/mark`, { chapters, done }),
  history: (limit = 30) => req<HistoryEntry[]>("GET", "/history" + qs({ limit })),
  removeHistory: (id: string) => req<void>("DELETE", `/history/${enc(id)}`),

  // libros
  books: () => req<Book[]>("GET", "/books"),
  book: (id: string) => req<Book>("GET", `/books/${enc(id)}`),
  editBook: (id: string, p: { title?: string; author?: string }) => req<Book>("PATCH", `/books/${enc(id)}`, p),
  deleteBook: (id: string) => req<void>("DELETE", `/books/${enc(id)}`),
  bookPages: (id: string) => req<{ pages: string[] }>("GET", `/books/${enc(id)}/pages`),
  bookFileUrl: (id: string) => `/api/books/${enc(id)}/file`,
  async setBookCover(id: string, blob: Blob) {
    const res = await fetch(`/api/books/${enc(id)}/cover`, { method: "PUT", body: blob, headers: { "Content-Type": blob.type } });
    if (!res.ok) throw new ApiError(res.status, "internal", t("common.errorStatus", { status: res.status }));
  },
  setBookPages: (id: string, pages: number) => req<void>("PUT", `/books/${enc(id)}/pages`, { pages }),
};

/** Sube archivos con progreso (fetch todavia no informa el progreso de subida). */
export function uploadBooks(files: File[], onProgress: (frac: number) => void): { done: Promise<Book[]>; abort: () => void } {
  const xhr = new XMLHttpRequest();
  const done = new Promise<Book[]>((resolve, reject) => {
    const fd = new FormData();
    for (const f of files) fd.append("file", f, f.name);
    xhr.open("POST", "/api/books");
    xhr.responseType = "json";
    xhr.upload.onprogress = (e) => {
      if (e.lengthComputable) onProgress(e.loaded / e.total);
    };
    xhr.onload = () => {
      if (xhr.status >= 200 && xhr.status < 300) resolve(xhr.response as Book[]);
      else {
        const j = (xhr.response ?? {}) as { error?: string; message?: string };
        if (xhr.status === 401) onUnauthorized?.();
        reject(new ApiError(xhr.status, j.error ?? "internal", j.message ?? t("common.errorStatus", { status: xhr.status })));
      }
    };
    xhr.onerror = () => reject(new ApiError(0, "unreachable", t("common.unreachable")));
    xhr.onabort = () => reject(new DOMException("aborted", "AbortError"));
    xhr.send(fd);
  });
  return { done, abort: () => xhr.abort() };
}

/** Busqueda global: el server manda una linea JSON por fuente apenas contesta (NDJSON). */
export async function searchAll(q: string, onGroup: (g: SearchGroup) => void, signal: AbortSignal, sources?: string[]): Promise<void> {
  const res = await req<Response>("GET", "/search" + qs({ q, sources: sources?.join(",") }), undefined, { signal, raw: true });
  const emit = (line: string) => {
    const s = line.trim();
    if (s) onGroup(JSON.parse(s) as SearchGroup);
  };
  if (!res.body) {
    (await res.text()).split("\n").forEach(emit);
    return;
  }
  const reader = res.body.pipeThrough(new TextDecoderStream()).getReader();
  let buf = "";
  for (;;) {
    const { value, done } = await reader.read();
    if (done) break;
    buf += value;
    let i: number;
    while ((i = buf.indexOf("\n")) >= 0) {
      emit(buf.slice(0, i));
      buf = buf.slice(i + 1);
    }
  }
  emit(buf);
}
