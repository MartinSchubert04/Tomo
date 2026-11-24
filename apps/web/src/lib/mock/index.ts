// Modo mock: un server de mentira dentro del navegador, solo para desarrollo (`npm run dev:mock`).
// Contesta docs/API.md con datos inventados: portadas y paginas son SVG generados, y hay un EPUB
// y un PDF de muestra (texto de dominio publico). El build de produccion no lo incluye.
// Usuario: cualquier nombre con la contraseña "mock" (admin si se llama "admin").
import epubUrl from "./sample.epub?url";
import pdfUrl from "./sample.pdf?url";
import type {
  Book,
  Chapter,
  Extension,
  HistoryEntry,
  LibraryEntry,
  LibraryStatus,
  MangaDetail,
  Progress,
  Source,
  User,
  WorkCard,
} from "../types";

const svg = (w: number, h: number, body: string) =>
  "data:image/svg+xml," + encodeURIComponent(`<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${w} ${h}" width="${w}" height="${h}">${body}</svg>`);

const HUES = [12, 200, 280, 140, 330, 40, 170, 230, 0, 260, 95, 310];
function cover(title: string, i: number): string {
  const h = HUES[i % HUES.length];
  const words = title.split(" ");
  const lines: string[] = [];
  for (const w of words) {
    const last = lines[lines.length - 1];
    if (last && (last + " " + w).length <= 11) lines[lines.length - 1] = last + " " + w;
    else lines.push(w);
  }
  const text = lines
    .map((l, k) => `<text x="22" y="${300 - (lines.length - 1 - k) * 34}" font-family="Georgia,serif" font-weight="700" font-size="30" fill="#fff">${l}</text>`)
    .join("");
  return svg(
    300,
    450,
    `<defs><linearGradient id="g" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="hsl(${h} 70% 45%)"/><stop offset="1" stop-color="hsl(${(h + 50) % 360} 60% 18%)"/></linearGradient></defs>
     <rect width="300" height="450" fill="url(#g)"/>
     <circle cx="${200 + (i % 3) * 20}" cy="${120 + (i % 4) * 15}" r="${60 + (i % 5) * 10}" fill="hsl(${(h + 180) % 360} 80% 70%)" opacity=".35"/>
     <path d="M0 ${330 + (i % 3) * 10} Q150 ${280 + (i % 4) * 20} 300 ${340} V450 H0Z" fill="#000" opacity=".35"/>
     ${text}<text x="22" y="420" font-family="sans-serif" font-size="14" fill="#fff" opacity=".7">VOL. ${1 + (i % 9)}</text>`,
  );
}

function page(n: number, chapter: number, tall: boolean, h: number): string {
  const W = 800;
  const H = tall ? 2200 : 1150;
  const panels = tall
    ? [0, 1, 2]
        .map((k) => `<rect x="60" y="${80 + k * 700}" width="680" height="600" rx="8" fill="hsl(${(h + k * 30) % 360} 30% 85%)" stroke="#222" stroke-width="5"/>`)
        .join("")
    : `<rect x="40" y="40" width="720" height="420" fill="hsl(${h} 25% 88%)" stroke="#111" stroke-width="5"/>
       <rect x="40" y="490" width="340" height="620" fill="hsl(${(h + 40) % 360} 25% 85%)" stroke="#111" stroke-width="5"/>
       <rect x="410" y="490" width="350" height="290" fill="hsl(${(h + 80) % 360} 25% 86%)" stroke="#111" stroke-width="5"/>
       <rect x="410" y="810" width="350" height="300" fill="hsl(${(h + 120) % 360} 25% 84%)" stroke="#111" stroke-width="5"/>
       <ellipse cx="560" cy="200" rx="140" ry="70" fill="#fff" stroke="#111" stroke-width="4"/>`;
  return svg(
    W,
    H,
    `<rect width="${W}" height="${H}" fill="#f7f5f0"/>${panels}
     <text x="${W / 2}" y="${tall ? 1100 : 700}" text-anchor="middle" font-family="sans-serif" font-weight="800" font-size="120" fill="#111" opacity=".18">${n}</text>
     <text x="${W - 30}" y="${H - 18}" text-anchor="end" font-family="sans-serif" font-size="22" fill="#555">cap. ${chapter} · ${n}</text>`,
  );
}

const icon = (letter: string, h: number) =>
  svg(64, 64, `<rect width="64" height="64" rx="14" fill="hsl(${h} 65% 45%)"/><text x="32" y="43" text-anchor="middle" font-family="sans-serif" font-weight="800" font-size="30" fill="#fff">${letter}</text>`);

const SOURCES: Source[] = [
  { id: "101", name: "InManga (ES)", lang: "es", icon: icon("I", 210), supportsLatest: true, nsfw: "safe" },
  { id: "102", name: "Olympus Scanlation (ES)", lang: "es", icon: icon("O", 30), supportsLatest: true, nsfw: "safe" },
  { id: "103", name: "MangaDex (ES-419)", lang: "es-419", icon: icon("M", 20), supportsLatest: true, nsfw: "mixed" },
  { id: "104", name: "LectorManga (ES)", lang: "es", icon: icon("L", 280), supportsLatest: false, nsfw: "safe" },
  { id: "105", name: "Webtoon Hub (ALL)", lang: "all", icon: icon("W", 140), supportsLatest: true, nsfw: "safe" },
];

const TITLES = [
  "La Torre del Alba",
  "Espada de Ceniza",
  "Café Medianoche",
  "El Último Dragón Azul",
  "Crónicas del Faro",
  "Ruta 66 Ninja",
  "Jardín de Invierno",
  "Kaiju en mi Escuela",
  "Rey de los Mares",
  "El Alquimista de Hielo",
  "Bajo la Luna Roja",
  "Ciudad de Papel",
  "Academia de Héroes Rotos",
  "Mil Años de Soledad",
  "El Gato del Templo",
  "Llamas Eternas",
  "Viaje a Cero",
  "Sombras del Norte",
  "Mi Vecino es un Demonio",
  "Pétalos de Acero",
  "Reencarnado como Biblioteca",
  "El Cartógrafo",
  "Ecos del Vacío",
  "Cocina Celestial",
  "Duelo en Neo Tokio",
  "La Bruja del Viento",
  "Estrella Fugaz",
  "Hermanos de Tinta",
  "El Jardinero Nocturno",
  "Marea Alta",
];
const GENRES = ["Acción", "Aventura", "Comedia", "Drama", "Fantasía", "Romance", "Sobrenatural", "Vida escolar", "Misterio", "Seinen", "Shounen"];

const works: WorkCard[] = TITLES.map((title, i) => ({
  id: `s${i + 1}`,
  kind: "manga",
  title,
  cover: cover(title, i),
  source: SOURCES[i % SOURCES.length].name,
}));

function chaptersOf(i: number): Chapter[] {
  const n = i === 0 ? 1184 : 20 + ((i * 37) % 140);
  const scans = ["Lector Scans", "Nocturno FS", "Traducciones Luna"];
  const now = Date.now();
  return Array.from({ length: n }, (_, k) => {
    const num = n - k;
    return {
      id: `${i + 1}${String(num).padStart(5, "0")}`,
      name: k % 9 === 2 ? `Capítulo ${num}: El regreso` : `Capítulo ${num}`,
      number: num,
      scanlator: scans[(num + i) % (i % 3 === 0 ? 3 : 1)],
      date: now - k * 3.5 * 86400000 - (i % 5) * 3600000,
    };
  });
}

function detail(id: string): MangaDetail | null {
  const i = Number(id.slice(1)) - 1;
  const w = works[i];
  if (!w) return null;
  return {
    ...w,
    kind: "manga",
    sourceId: SOURCES[i % SOURCES.length].id,
    author: i % 4 === 3 ? null : ["Aiko Tanabe", "Min-jun Park", "Hiro Sato", "Lucía Ferro"][i % 4],
    artist: i % 3 === 0 ? "Kenji Mori" : null,
    description:
      "Cuando la torre aparece de la noche a la mañana en el centro de la ciudad, nadie sabe quién la construyó ni qué hay arriba. " +
      "Rin, una estudiante que no cree en nada, encuentra una llave con su nombre grabado y empieza a subir piso por piso, " +
      "cada uno con sus reglas, sus monstruos y sus promesas. Pero cuanto más sube, menos recuerda de dónde vino.\n\n" +
      "Una historia de aventura y misterio con un dibujo detallado y personajes que crecen capítulo a capítulo.",
    genres: GENRES.filter((_, k) => (k + i) % 3 === 0).slice(0, 5),
    status: (["ongoing", "completed", "hiatus", "ongoing"] as const)[i % 4],
    url: "https://example.com/manga/" + id,
    chapters: chaptersOf(i),
  };
}

const now = Date.now();
const books: Book[] = [
  { id: "b1", kind: "book", title: "Don Quijote (muestra)", author: "Miguel de Cervantes", format: "epub", cover: cover("Don Quijote", 5), size: 31541, addedAt: now - 3 * 86400000, uploader: "admin", pages: null },
  { id: "b2", kind: "book", title: "Apuntes de muestra", author: null, format: "pdf", cover: null, size: 8561, addedAt: now - 5 * 86400000, uploader: "martin", pages: null },
  { id: "b3", kind: "book", title: "Kaiju en mi Escuela Vol. 1", author: "Hiro Sato", format: "cbz", cover: cover("Kaiju Vol. 1", 7), size: 48_200_000, addedAt: now - 9 * 86400000, uploader: "admin", pages: 24 },
  { id: "b4", kind: "book", title: "El Arte de Leer Despacio", author: "Ana Ruiz", format: "mobi", cover: cover("El Arte de Leer", 2), size: 1_200_000, addedAt: now - 12 * 86400000, uploader: "admin", pages: null },
  { id: "b5", kind: "book", title: "Cuentos de la Selva", author: "Horacio Quiroga", format: "epub", cover: cover("Cuentos de la Selva", 3), size: 640_000, addedAt: now - 20 * 86400000, uploader: "martin", pages: null },
  { id: "b6", kind: "book", title: "Diario de Viaje", author: "Anónimo", format: "txt", cover: null, size: 12_000, addedAt: now - 30 * 86400000, uploader: "admin", pages: null },
];

const db = {
  users: [
    { id: 1, name: "admin", admin: true, createdAt: now - 40 * 86400000 },
    { id: 2, name: "martin", admin: false, createdAt: now - 20 * 86400000 },
  ] as User[],
  me: null as User | null,
  prefs: {} as Record<string, unknown>,
  library: new Map<string, LibraryEntry>(),
  progress: new Map<string, Record<string, Progress>>(),
  last: new Map<string, Progress>(),
  repos: ["https://raw.githubusercontent.com/keiyoushi/extensions/repo/index.min.json"],
};

// Biblioteca y progreso de arranque (para que el inicio tenga algo).
for (const [i, status, unread] of [
  [0, "reading", 3],
  [2, "reading", 1],
  [4, "planned", 0],
  [7, "completed", 0],
  [9, "reading", 12],
  [13, "onhold", 0],
] as [number, LibraryStatus, number][]) {
  const w = works[i];
  db.library.set(w.id, { work: w, status, favorite: i === 2, addedAt: now - i * 86400000, lastReadAt: now - i * 3600000, unread, latestChapterAt: now - i * 7200000 });
}
function seedProgress(id: string, chapter: string, page: number, pages: number, ago: number) {
  const p: Progress = { chapter, position: String(page), fraction: (page + 1) / pages, done: false, updatedAt: now - ago };
  db.last.set(id, p);
  const d = detail(id);
  const map: Record<string, Progress> = {};
  for (const c of d?.chapters ?? []) if (Number(c.id) < Number(chapter)) map[c.id] = { chapter: c.id, position: "17", fraction: 1, done: true, updatedAt: now - ago - 86400000 };
  map[chapter] = p;
  db.progress.set(id, map);
}
seedProgress("s1", "101180", 7, 18, 1800000);
seedProgress("s3", "300040", 12, 18, 7200000);
seedProgress("s10", "1000020", 3, 18, 26 * 3600000);
db.last.set("b1", { chapter: "", position: "", fraction: 0.32, done: false, updatedAt: now - 5 * 3600000 });
db.last.set("b3", { chapter: "", position: "9", fraction: 10 / 24, done: false, updatedAt: now - 50 * 3600000 });

const EXT_NAMES = ["InManga", "Olympus Scanlation", "MangaDex", "LectorManga", "Webtoon Hub", "TuManhwas", "Ikigai Mangas", "Manga Crab", "LeerCapitulo", "Mangas.in", "Bokugen", "Dragon Translation", "EternalMangas", "HeavenManga", "Kumanga", "LectorJPG", "MangaOni", "Manhwa-Latino", "Mantraz Scan", "NekoScans", "Ragna Scans", "Ritto", "SkyMangas", "Taurus Fansub", "Toon-es", "Yupmanga", "Asura Scans", "Comick", "Manganato", "Bato.to", "Webtoons.com", "Tapas"];
let exts: Extension[] = EXT_NAMES.map((name, i) => ({
  pkg: `eu.kanade.tachiyomi.extension.${i < 26 ? "es" : i < 28 ? "en" : "all"}.${name.toLowerCase().replace(/[^a-z]/g, "")}`,
  name,
  lang: i === 2 ? "all" : i < 26 ? "es" : i < 28 ? "en" : "all",
  version: `1.4.${10 + i}`,
  icon: icon(name[0], (i * 47) % 360),
  installed: i < 5,
  hasUpdate: i === 1,
  obsolete: i === 23,
  nsfw: i % 7 === 3 ? "nsfw" : i % 5 === 2 ? "mixed" : "safe",
}));

// ---------- helpers ----------
const json = (data: unknown, status = 200) =>
  new Response(JSON.stringify(data), { status, headers: { "Content-Type": "application/json" } });
const empty = () => new Response(null, { status: 204 });
const err = (status: number, error: string, message = error) => json({ error, message }, status);
const wait = (ms: number) => new Promise((r) => setTimeout(r, ms));

function historyEntries(): HistoryEntry[] {
  const out: HistoryEntry[] = [];
  for (const [id, p] of db.last) {
    const w = works.find((x) => x.id === id) ?? books.find((b) => b.id === id);
    if (!w) continue;
    let chapterName: string | null = null;
    let nextChapter: string | null = null;
    if (w.kind === "manga") {
      const d = detail(id)!;
      const i = d.chapters.findIndex((c) => c.id === p.chapter);
      chapterName = i >= 0 ? `Cap. ${d.chapters[i].number}` : null;
      if (p.done && i > 0) nextChapter = d.chapters[i - 1].id;
    }
    out.push({ work: w, progress: p, chapterName, nextChapter });
  }
  return out.sort((a, b) => b.progress.updatedAt - a.progress.updatedAt);
}

function browse(sourceId: string, type: string, pageN: number, q: string) {
  const si = SOURCES.findIndex((s) => s.id === sourceId);
  let list = works.filter((_, i) => (i + si) % 2 === 0 || type !== "latest");
  if (type === "latest") list = [...list].reverse();
  if (q) list = works.filter((w) => w.title.toLowerCase().includes(q.toLowerCase()));
  // Paginas de 16 (para probar el scroll infinito).
  const per = 16;
  const items = list.slice((pageN - 1) * per, pageN * per);
  return { items, hasNext: pageN * per < list.length };
}

async function handle(method: string, path: string, url: URL, body: unknown): Promise<Response> {
  await wait(120 + Math.random() * 280);
  const seg = path.split("/").filter(Boolean); // ["auth","login"]
  if (path === "/health") return json({ ok: true, name: "mock", version: "0.0.0", suwayomi: true });
  if (path === "/auth/login" && method === "POST") {
    const b = body as { name: string; password: string };
    if (b.password !== "mock") return err(401, "unauthorized", "Wrong user name or password");
    db.me = db.users.find((u) => u.name === b.name) ?? { id: 99, name: b.name, admin: b.name === "admin" };
    localStorage.setItem("library.mock.user", db.me.name);
    return json({ user: db.me });
  }
  if (!db.me) {
    const n = localStorage.getItem("library.mock.user");
    if (n) db.me = db.users.find((u) => u.name === n) ?? { id: 99, name: n, admin: n === "admin" };
  }
  if (!db.me) return err(401, "unauthorized");
  if (path === "/auth/logout") {
    db.me = null;
    localStorage.removeItem("library.mock.user");
    return empty();
  }
  if (path === "/me") return json({ user: db.me, prefs: db.prefs });
  if (path === "/me/prefs") return (db.prefs = body as Record<string, unknown>), empty();
  if (path === "/me/password") return (body as { current: string }).current === "mock" ? empty() : err(401, "unauthorized");

  // admin
  if (seg[0] === "admin") {
    if (!db.me.admin) return err(403, "forbidden");
    if (path === "/admin/users" && method === "GET") return json(db.users);
    if (path === "/admin/users" && method === "POST") {
      const b = body as { name: string; admin: boolean };
      if (db.users.some((u) => u.name === b.name)) return err(409, "conflict");
      const u = { id: db.users.length + 10, name: b.name, admin: b.admin, createdAt: Date.now() };
      db.users.push(u);
      return json(u);
    }
    if (seg[1] === "users" && seg[2]) {
      const id = Number(seg[2]);
      if (method === "DELETE") db.users = db.users.filter((u) => u.id !== id);
      if (method === "PATCH" && (body as { admin?: boolean }).admin !== undefined)
        db.users = db.users.map((u) => (u.id === id ? { ...u, admin: !!(body as { admin: boolean }).admin } : u));
      return empty();
    }
    if (path === "/admin/extensions") return json([...exts].sort((a, b) => Number(b.installed) - Number(a.installed) || a.name.localeCompare(b.name)));
    if (path === "/admin/extensions/refresh") return await wait(800), json(exts);
    if (seg[1] === "extensions" && seg[2]) {
      await wait(900);
      const pkg = decodeURIComponent(seg[2]);
      const a = (body as { action: string }).action;
      exts = exts.map((e) => (e.pkg === pkg ? { ...e, installed: a !== "uninstall", hasUpdate: false } : e));
      return json(exts.find((e) => e.pkg === pkg));
    }
    if (path === "/admin/repos") return method === "PUT" ? ((db.repos = body as string[]), empty()) : json(db.repos);
  }

  if (path === "/sources") return json(SOURCES);
  if (seg[0] === "sources" && seg[2] === "browse") {
    return json(browse(seg[1], url.searchParams.get("type") ?? "popular", Number(url.searchParams.get("page") ?? 1), url.searchParams.get("q") ?? ""));
  }
  if (path === "/search") {
    const q = (url.searchParams.get("q") ?? "").toLowerCase();
    const only = url.searchParams.get("sources")?.split(",");
    const list = SOURCES.filter((s) => !only || only.includes(s.id));
    const enc = new TextEncoder();
    const stream = new ReadableStream({
      async start(c) {
        for (const [k, s] of list.entries()) {
          await wait(300 + k * 350);
          const items = k === 3 ? [] : works.filter((w, i) => w.title.toLowerCase().includes(q) || (i + k) % 7 === 0).slice(0, 12);
          const line = k === 4 ? { source: s, items: [], error: "timeout" } : { source: s, items };
          c.enqueue(enc.encode(JSON.stringify(line) + "\n"));
        }
        c.close();
      },
    });
    return new Response(stream, { headers: { "Content-Type": "application/x-ndjson" } });
  }

  if (seg[0] === "manga") {
    const d = detail(seg[1]);
    if (!d) return err(404, "not_found");
    if (seg[2] === "chapters" && seg[4] === "pages") {
      const ch = Number(seg[3]) % 100000;
      const i = Number(seg[1].slice(1)) - 1;
      const tall = i % 5 === 4;
      return json({ pages: Array.from({ length: tall ? 10 : 18 }, (_, k) => page(k + 1, ch, tall, (i * 30 + k * 7) % 360)) });
    }
    return json(d);
  }

  if (path === "/library") return json([...db.library.values()].sort((a, b) => (b.lastReadAt ?? 0) - (a.lastReadAt ?? 0)));
  if (seg[0] === "library" && seg[1]) {
    const id = decodeURIComponent(seg[1]);
    if (method === "DELETE") return db.library.delete(id), empty();
    const w = works.find((x) => x.id === id) ?? books.find((b) => b.id === id)!;
    const cur = db.library.get(id) ?? { work: w, status: "reading" as LibraryStatus, favorite: false, addedAt: Date.now(), lastReadAt: null, unread: w.kind === "manga" ? 0 : null, latestChapterAt: null };
    const n = { ...cur, ...(body as object) };
    db.library.set(id, n);
    return json(n);
  }
  if (seg[0] === "progress" && seg[1]) {
    const id = decodeURIComponent(seg[1]);
    const map = db.progress.get(id) ?? {};
    if (seg[2] === "mark") {
      const b = body as { chapters: string[]; done: boolean };
      for (const c of b.chapters) map[c] = { chapter: c, position: "0", fraction: b.done ? 1 : 0, done: b.done, updatedAt: Date.now() };
      db.progress.set(id, map);
      return empty();
    }
    if (method === "PUT") {
      const p = { ...(body as Progress), updatedAt: Date.now() };
      if (p.chapter) map[p.chapter] = p;
      db.progress.set(id, map);
      db.last.set(id, p);
      return empty();
    }
    return json({ last: db.last.get(id) ?? null, chapters: map });
  }
  if (path === "/history") return json(historyEntries());
  if (seg[0] === "history" && seg[1]) return db.last.delete(decodeURIComponent(seg[1])), empty();

  if (path === "/books") return json(books);
  if (seg[0] === "books" && seg[1]) {
    const b = books.find((x) => x.id === seg[1]);
    if (!b) return err(404, "not_found");
    if (seg[2] === "file") {
      if (b.format === "pdf") return fetchReal(pdfUrl);
      if (b.format === "txt") return new Response("Diario de viaje\n\n" + "Salimos temprano por la ruta, con el sol de frente y el mate pasando de mano en mano. ".repeat(400), { headers: { "Content-Type": "text/plain" } });
      if (b.format === "epub" || b.format === "mobi") return fetchReal(epubUrl);
      return err(404, "not_found");
    }
    if (seg[2] === "pages" && method === "GET") return json({ pages: Array.from({ length: 24 }, (_, k) => page(k + 1, 1, false, (k * 13) % 360)) });
    if (seg[2] === "pages" || seg[2] === "cover") return empty();
    if (method === "PATCH") return Object.assign(b, body), json(b);
    if (method === "DELETE") return books.splice(books.indexOf(b), 1), empty();
    return json(b);
  }
  return err(404, "not_found", "mock: " + path);
}

let realFetch: typeof fetch;
const fetchReal = (u: string) => realFetch(u);

export function installMock() {
  realFetch = window.fetch.bind(window);
  window.fetch = async (input: RequestInfo | URL, init?: RequestInit) => {
    const req = input instanceof Request ? input : null;
    const url = new URL(req ? req.url : String(input), location.href);
    if (url.origin !== location.origin || !url.pathname.startsWith("/api/")) return realFetch(input, init);
    const method = (init?.method ?? req?.method ?? "GET").toUpperCase();
    let body: unknown = undefined;
    const raw = init?.body;
    if (typeof raw === "string") {
      try {
        body = JSON.parse(raw);
      } catch {
        body = raw;
      }
    }
    return handle(method, url.pathname.slice(4), url, body);
  };
  console.info("[mock] API de mentira activa: cualquier usuario con la contraseña 'mock'");
}
