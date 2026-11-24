# API

HTTP contract between the interface (`apps/web`) and the server (`backend/server`, binary
`library`). JSON in and out, `camelCase` fields. Every `/api` route needs a session except
`GET /api/health` and `POST /api/auth/login`.

## Conventions

- **Session.** `POST /api/auth/login` sets the `lib_session` cookie (HttpOnly, SameSite=Lax,
  Path=/, 180 days, renewed on use). The server also accepts `Authorization: Bearer <token>`
  (for future native apps). `<img>` and `fetch` from the web send the cookie on their own, so
  image URLs carry no token.
- **Errors.** Status code + `{ "error": "<code>", "message": "<human text, English>" }`. Codes:
  `unauthorized` (401), `forbidden` (403), `not_found` (404), `bad_request` (400),
  `conflict` (409), `too_large` (413), `rate_limited` (429), `source_error` (502: the manga
  source or Suwayomi failed), `unavailable` (503: Suwayomi not reachable), `internal` (500).
  The interface translates by `error` code (`lib/i18n/server.ts`).
- **Work ids.** Everything readable is a *work* with a string id:
  - `s<int>`: a manga from Suwayomi (its manga id), e.g. `s1234`.
  - `b<int>`: an uploaded book or comic file, e.g. `b17`.
- **Chapter ids.** Manga: Suwayomi chapter id as a string (`"1201"`). Books have no chapters
  (`chapter` is `""`).
- **Dates** are Unix milliseconds (numbers).
- **Images** are always same-origin URLs returned by the server (`/api/img/...`,
  `/api/books/{id}/cover`). The interface never builds image URLs on its own.

## Shared shapes

```ts
type User = { id: number; name: string; admin: boolean };

type WorkCard = {
  id: string;             // "s123" | "b4"
  kind: "manga" | "book";
  title: string;
  cover: string | null;   // URL
  source?: string;        // display name of the source ("InManga (ES)") or "Upload"
  format?: BookFormat;    // books only
};

type BookFormat = "epub" | "pdf" | "cbz" | "mobi" | "azw3" | "fb2" | "txt";

type Chapter = {
  id: string;
  name: string;           // as the source gives it ("Capítulo 12: ...")
  number: number;         // -1 if unknown
  scanlator: string | null; // without the language prefix ("[es-la] Grupo" → "Grupo")
  lang: string | null;    // "es-419", "en"... when the source says it (multi-language sources put
                          // it as a "[xx]" prefix of the scanlator; es-la is normalized to es-419);
                          // null = the source's language
  date: number;           // upload date, ms (0 if unknown)
};

type MangaDetail = WorkCard & {
  kind: "manga";
  sourceId: string;
  author: string | null;
  artist: string | null;
  description: string | null;
  genres: string[];
  status: "ongoing" | "completed" | "hiatus" | "cancelled" | "unknown";
  url: string | null;     // page on the original site
  chapters: Chapter[];    // newest first. The same number can appear more than once (one per
                          // language or scan group): the UI shows one row per number
};

type Book = WorkCard & {
  kind: "book";
  format: BookFormat;
  author: string | null;
  size: number;           // bytes
  addedAt: number;
  uploader: string;       // user name
  pages: number | null;   // cbz: page count; pdf: filled by the client after first open
  series: string | null;  // from the EPUB (calibre:series or EPUB 3 belongs-to-collection) or edited
  seriesIndex: number | null;
  language: string | null; // "es", "en"... (normalized: spa/Spanish/es-AR → es)
  description: string | null;
  year: number | null;
  origin: string | null;  // catalog name, "inbox", or null (uploaded)
};

type LibraryStatus = "reading" | "planned" | "completed" | "onhold" | "dropped";

type Progress = {
  chapter: string;        // "" for books
  position: string;       // manga: page index as string ("12"); epub: CFI; pdf/cbz: page index
  fraction: number;       // 0..1 inside the chapter (manga) or the whole book (books)
  done: boolean;
  updatedAt: number;
};
```

## Health and session

| Method | Route | Body | Response |
|---|---|---|---|
| GET | `/api/health` | | `{ ok: true, name, version, suwayomi: boolean }` |
| POST | `/api/auth/login` | `{ name, password }` | `{ user }` + cookie. 401 `unauthorized` if wrong; 429 after 8 failures per IP in 10 min |
| POST | `/api/auth/logout` | | 204, clears the cookie and deletes the session |
| GET | `/api/me` | | `{ user, prefs }` (`prefs`: free-form JSON object saved by the UI, `{}` at first) |
| PUT | `/api/me/prefs` | any JSON object (≤ 64 KB) | 204 |
| POST | `/api/me/password` | `{ current, next }` (next ≥ 6 chars) | 204; 401 if `current` is wrong. Other sessions of the user are closed |

## Admin (admins only, 403 otherwise)

| Method | Route | Body | Response |
|---|---|---|---|
| GET | `/api/admin/users` | | `User[]` (+ `createdAt`) |
| POST | `/api/admin/users` | `{ name, password, admin }` | `User`; 409 if the name exists |
| PATCH | `/api/admin/users/{id}` | `{ password?, admin? }` | 204 (can't remove the last admin: 409) |
| DELETE | `/api/admin/users/{id}` | | 204 (not yourself: 409) |
| GET | `/api/admin/extensions?lang=es&q=&installed=1` | | `Extension[]` sorted installed first, then by name. `lang` takes a comma list (`es,all`); `es` also matches `es-419` |
| POST | `/api/admin/extensions/refresh` | | `Extension[]` (re-reads the repo index) |
| POST | `/api/admin/extensions/{pkg}` | `{ action: "install" \| "uninstall" \| "update" }` | `Extension` |
| GET | `/api/admin/repos` | | `string[]` (extension repo index URLs) |
| PUT | `/api/admin/repos` | `string[]` | 204 |

```ts
type Extension = {
  pkg: string; name: string; lang: string; version: string;
  icon: string;           // /api/img/s/extension/icon/<pkg>
  installed: boolean; hasUpdate: boolean; obsolete: boolean;
  nsfw: "safe" | "mixed" | "nsfw";
};
```

## Sources and browsing (manga)

| Method | Route | Response |
|---|---|---|
| GET | `/api/sources` | `Source[]`: installed sources, without "Local source" |
| GET | `/api/sources/{sourceId}/browse?type=popular\|latest\|search&q=&page=1` | `{ items: WorkCard[], hasNext: boolean }` |
| GET | `/api/search?q=&sources=id1,id2&alt=&alt=&resolve=1` | **NDJSON stream** (`application/x-ndjson`): one line per source as soon as it answers, `{ source: Source, items: WorkCard[], error?: string }`. Without `sources`: the user's pinned sources (`prefs.sources`), or else every installed source whose lang is `es`, `es-419` or `all`, at most 12 (the UI sends the ones of the user's source languages). Each source has 15 s; 6 in parallel. **Other titles:** `alt` (repeatable) are other names of the same work (from `/api/suggest`); each source is searched with `q` first and then the alts, at most 3 searches, results merged without repeats (by work id) and ordered by how well the title matches any of them (exact, then starts with, then contains, then the source's order), `error` only if all failed. Without `alt`, `resolve=1` asks AniList (1 s at most) whether `q` is the name of a work in another language and, if so, uses its titles. When other titles were used, the **first line** is `{ alts: string[] }` (the titles besides `q`) |
| GET | `/api/suggest?q=` | `Suggestion[]` while typing (≥ 2 chars): first the known cards (any manga someone opened, added or read, and uploaded books) that match, the user's library and history first; then AniList titles: one that matches a known card becomes that card (with its other titles), the rest come as `kind: "title"`. Never fails because of AniList (cached 24 h per query, ≤ 25 requests per minute for the whole server, 2.5 s timeout); adult works only with `prefs.nsfw` |
| GET | `/api/img/al/{path}` | AniList covers through the server (`https://s4.anilist.co/{path}`, only `file/anilistcdn/...`), `Cache-Control: private, max-age=31536000, immutable`. The browser never talks to AniList |

```ts
type Source = { id: string; name: string; lang: string; icon: string; supportsLatest: boolean; nsfw: "safe" | "mixed" | "nsfw" };

type Suggestion = {
  kind: "work" | "title";   // work: exists here (open it); title: search the sources with its names
  workId?: string;          // kind "work"
  title: string;            // the card's title, or AniList's romaji
  altTitles: string[];      // other names (romaji, English, synonyms, native)
  cover: string | null;     // /api/img/s/..., /api/books/{id}/cover or /api/img/al/...
  format: "manga" | "manhwa" | "manhua" | "novel" | BookFormat | null;
  year: number | null;
  source?: string;          // kind "work": the source name or "Upload"
};
```

## Manga

| Method | Route | Response |
|---|---|---|
| GET | `/api/manga/{id}` | `MangaDetail`. Uses what Suwayomi already has; asks the source only if it was never fetched or it's older than 6 h |
| GET | `/api/manga/{id}?refresh=1` | `MangaDetail` asking the source again (pull to refresh) |
| GET | `/api/manga/{id}/chapters/{chapterId}/pages` | `{ pages: string[] }` image URLs, in order |
| GET | `/api/img/s/{path}` | proxy to Suwayomi `GET /api/v1/{path}` (covers, pages, extension icons). Streams the body, copies `Content-Type`, adds `Cache-Control: private, max-age=31536000, immutable` (pages and icons) or `max-age=86400` (thumbnails). Only paths that start with `manga/` or `extension/icon/` |

## Library, progress and history (per user)

| Method | Route | Body | Response |
|---|---|---|---|
| GET | `/api/library` | | `LibraryEntry[]`, most recently read first |
| PUT | `/api/library/{workId}` | `{ status?, favorite? }` | `LibraryEntry` (adds it if it wasn't there; default status `reading`) |
| DELETE | `/api/library/{workId}` | | 204 |
| GET | `/api/progress/{workId}` | | `{ last: Progress \| null, chapters: Record<chapterId, Progress> }` |
| PUT | `/api/progress/{workId}` | `Progress` without `updatedAt` | 204. Saves per chapter and as `last`. The UI sends it at most every 3 s while reading and on leaving |
| POST | `/api/progress/{workId}/mark` | `{ chapters: string[], done: boolean }` | 204 (mark read / unread in bulk) |
| GET | `/api/history?limit=30` | | `HistoryEntry[]`, newest first, one per work |
| DELETE | `/api/history/{workId}` | | 204 (removes it from "Continue reading"; keeps the read marks) |

```ts
type LibraryEntry = {
  work: WorkCard;
  status: LibraryStatus;
  favorite: boolean;
  addedAt: number;
  lastReadAt: number | null;
  unread: number | null;          // manga: chapter numbers not read (any version read counts); books: null
  newChapters: number | null;     // manga: unread numbers that arrived after it was added or last read
                                  // (max(upload date, first seen) > max(addedAt, last read)); books: null
  latestChapterAt: number | null; // manga: date of the newest chapter
};

type HistoryEntry = {
  work: WorkCard;
  progress: Progress;
  chapterName: string | null;     // manga
  nextChapter: string | null;     // manga: id of the next chapter number if this one is done (same
                                  // language and scan group when that number has them)
};
```

The server keeps a copy of each work's card (`title`, `cover`, `source`) when it's added to the
library or read, so the library and history load without asking Suwayomi.

**Library updates.** Every 6 h (and 30 s after starting) the server asks for the chapters of every
manga in anyone's library (3 at a time). `unread`, `newChapters` and `latestChapterAt` come from
there. Each chapter remembers when it was first seen: the ones of the first fetch count as already
there, so adding a manga with 400 chapters doesn't make 400 "new" ones.

## Books (shared by all users: uploads, catalog imports and the inbox folder)

| Method | Route | Body | Response |
|---|---|---|---|
| GET | `/api/books` | | `Book[]`, newest first |
| POST | `/api/books` | `multipart/form-data`, one or more `file` fields (≤ 1 GB each) | `Book[]`. Format by extension (and magic bytes for zip/pdf). `.zip` and `.cbz` = `cbz`. EPUB: title, author, cover, series and number, language, synopsis and year from the OPF. CBZ: cover = first image (natural sort). Others: title = file name. Afterwards, in the background, Open Library fills whatever is still missing (author, year, language, synopsis, cover); it never overwrites what the file had |
| GET | `/api/books/{id}` | | `Book` |
| PATCH | `/api/books/{id}` | `{ title?, author?, series?, seriesIndex?, language?, description?, year? }` (`""` clears a text field; `series: ""` also clears the number; `year: 0` clears it) | `Book` (uploader or admin; 403 otherwise) |
| DELETE | `/api/books/{id}` | | 204 (uploader or admin) |
| GET | `/api/books/{id}/file` | | the file, with `Range` support and its `Content-Type` |
| GET | `/api/books/{id}/cover` | | image (404 if none). `Cache-Control: private, max-age=86400` |
| PUT | `/api/books/{id}/cover` | image body (`image/jpeg`, `image/webp` or `image/png`, ≤ 2 MB) | 204. Used by the UI to save the first page of a PDF as its cover |
| PUT | `/api/books/{id}/pages` | `{ pages: number }` | 204 (the UI reports a PDF's page count) |
| GET | `/api/books/{id}/pages` | | `{ pages: string[] }` (cbz only) |
| GET | `/api/books/{id}/pages/{n}` | | the n-th image inside the cbz (natural sort, images only) |

## Book catalogs (free and legal only)

Where to get books besides uploading them. Only public domain or openly licensed libraries; never
shadow libraries. Built-in ("preset") catalogs, Spanish first:

| id | What | How |
|---|---|---|
| `elejandria` | Elejandría: ~2.400 free and legal ebooks in Spanish | No API: the book list comes from its `sitemap.xml` and its per-letter lists (`/libros/a`…), refreshed daily and searched in memory (its `/buscar` is disallowed by `robots.txt`); a book's page gives the synopsis, cover and download link |
| `textos` | textos.info: ~6.000 Spanish public domain texts | OPDS |
| `archive` | Internet Archive, Spanish texts that are verified public domain (`collection:gutenberg` or `possible-copyright-status:NOT_IN_COPYRIGHT`), never lending items (`access-restricted-item`), with an EPUB or text PDF | `advancedsearch.php`, `metadata/<id>`, `download/<id>/<file>` |
| `gutenberg` | Project Gutenberg (mostly English; its Spanish books are mirrored on Internet Archive). **Off by default** | OPDS |

Each one can be switched off with an empty `LIBRARY_*_URL` (see `deploy/.env.example`). Admins
add any other OPDS catalog by URL (their Calibre, Kavita, a public library).

```ts
type Catalog = { id: string; name: string; kind: "opds" | "archive" | "elejandria"; url?: string /* admins */; enabled: boolean; preset: boolean; spanish: boolean };
type CatalogEntry = {
  id: string;             // opaque: send it back to import or get the detail
  title: string; author: string | null; cover: string | null /* same-origin URL */;
  language: string | null; year: number | null; summary: string | null;
  formats: string[];      // "epub", "pdf"... downloadable ones
  imported: string | null; // "b12" if it's already in the books
  url: string | null;     // the book's page on the original site
};
type CatalogPage = { title: string | null; entries: CatalogEntry[]; navigation: { title: string; path: string }[]; next: string | null };
```

| Method | Route | Body | Response |
|---|---|---|---|
| GET | `/api/catalogs` | | `Catalog[]` (enabled ones, Spanish first, no `url`) |
| GET | `/api/catalogs/{id}/browse?q=&path=&lang=all` | | `CatalogPage`. Without `q`: the popular/newest books (an OPDS root that only has sections opens its first one, and keeps the sections in `navigation`). `path`: a `navigation[].path` or `next` from a previous page (opaque, signed by the server). `lang=all`: don't filter to Spanish (by default, OPDS entries that say another language and non-Spanish Internet Archive items are left out) |
| GET | `/api/catalogs/{id}/entry/{entryId}` | | `CatalogEntry` with the synopsis and real title (Elejandría, Internet Archive; 404 for OPDS, whose list already has everything) |
| GET | `/api/catalogs/search?q=&lang=all` | | **NDJSON**, one line per enabled catalog as soon as it answers: `{ catalog: Catalog, entries: CatalogEntry[], error?: string }`. 15 s per catalog, 6 at a time |
| POST | `/api/catalogs/{id}/import` | `{ entry: string, hint?: { title, author, language, summary, year } }` | `Book`: **201** if it was downloaded now, **200** if it was already there (same source URL). The file is streamed to disk (≤ `LIBRARY_MAX_UPLOAD_MB`) and checked (zip/PDF magic bytes). `hint` (what the list showed) fills what the file doesn't have. 403 for Internet Archive lending items, 400 for an entry the server didn't sign |
| GET | `/api/catalogs/{id}/cover/{entryId}` | | the cover, looked up on the book's page (Elejandría books not in its lists) |
| GET | `/api/img/cat?t=` | | proxy for catalog covers. Only URLs the server signed (HMAC with a key made at every start): it's never an open proxy. `Cache-Control: private, max-age=604800` |

Admin (403 otherwise):

| Method | Route | Body | Response |
|---|---|---|---|
| GET | `/api/admin/catalogs` | | `Catalog[]` with `url`, including disabled ones |
| POST | `/api/admin/catalogs` | `{ url, name? }` | `Catalog`. The URL is fetched first: 400 if it isn't an OPDS feed, 409 if it's already there. Name: the feed's title |
| PATCH | `/api/admin/catalogs/{id}` | `{ enabled?, name? }` | 204 (presets can be turned off but not renamed) |
| DELETE | `/api/admin/catalogs/{id}` | | 204; 409 for presets (turn them off instead) |

## Inbox folder

Files left in `<LIBRARY_DATA>/inbox` (by hand, SFTP, Syncthing) are imported every
`LIBRARY_INBOX_SECONDS` (30): only when a file stopped changing (same size as the previous check
and untouched for 10 s). Partial downloads (`.part`, `.tmp`, `.crdownload`, Syncthing temp files)
and hidden files are skipped. Files that can't be imported are moved to `inbox/failed/` with a
`<name>.error.txt` next to them. Imported books get `uploader: "inbox"`, `origin: "inbox"`.

| Method | Route | Response |
|---|---|---|
| GET | `/api/admin/inbox` | `{ path, hostPath, pending: string[], failed: { name, error }[] }` (`hostPath` from `LIBRARY_INBOX_LABEL`) |
| POST | `/api/admin/inbox/scan` | the same, after checking now (two passes, without waiting the 10 s) |

