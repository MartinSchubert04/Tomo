// Las formas que viajan entre la interfaz y el server. Copia de docs/API.md: si cambia ahi,
// cambia aca.

export type User = { id: number; name: string; admin: boolean; createdAt?: number };

export type BookFormat = "epub" | "pdf" | "cbz" | "mobi" | "azw3" | "fb2" | "txt";

export type WorkCard = {
  id: string;
  kind: "manga" | "book";
  title: string;
  cover: string | null;
  source?: string;
  format?: BookFormat;
};

export type Chapter = {
  id: string;
  name: string;
  number: number;
  scanlator: string | null;
  /** Idioma del capitulo si la fuente lo dice ("es-419", "en"); null = el de la fuente. */
  lang: string | null;
  date: number;
};

export type MangaStatus = "ongoing" | "completed" | "hiatus" | "cancelled" | "unknown";

export type MangaDetail = WorkCard & {
  kind: "manga";
  sourceId: string;
  author: string | null;
  artist: string | null;
  description: string | null;
  genres: string[];
  status: MangaStatus;
  url: string | null;
  chapters: Chapter[];
};

export type Book = WorkCard & {
  kind: "book";
  format: BookFormat;
  author: string | null;
  size: number;
  addedAt: number;
  uploader: string;
  pages: number | null;
};

export type LibraryStatus = "reading" | "planned" | "completed" | "onhold" | "dropped";
export const LIBRARY_STATUSES: LibraryStatus[] = ["reading", "planned", "completed", "onhold", "dropped"];

export type Progress = {
  chapter: string;
  position: string;
  fraction: number;
  done: boolean;
  updatedAt: number;
};

export type LibraryEntry = {
  work: WorkCard;
  status: LibraryStatus;
  favorite: boolean;
  addedAt: number;
  lastReadAt: number | null;
  unread: number | null;
  /** Sin leer que llegaron despues de agregarlo o de la ultima lectura. */
  newChapters: number | null;
  latestChapterAt: number | null;
};

export type HistoryEntry = {
  work: WorkCard;
  progress: Progress;
  chapterName: string | null;
  nextChapter: string | null;
};

export type Nsfw = "safe" | "mixed" | "nsfw";

export type Source = { id: string; name: string; lang: string; icon: string; supportsLatest: boolean; nsfw: Nsfw };

export type Extension = {
  pkg: string;
  name: string;
  lang: string;
  version: string;
  icon: string;
  installed: boolean;
  hasUpdate: boolean;
  obsolete: boolean;
  nsfw: Nsfw;
};

export type BrowseType = "popular" | "latest" | "search";
export type BrowsePage = { items: WorkCard[]; hasNext: boolean };
export type SearchGroup = { source: Source; items: WorkCard[]; error?: string };

export type SuggestFormat = "manga" | "manhwa" | "manhua" | "novel";
export type Suggestion = {
  kind: "work" | "title";
  workId?: string;
  title: string;
  altTitles: string[];
  cover: string | null;
  format: SuggestFormat | BookFormat | null;
  year: number | null;
  source?: string;
};

export type WorkProgress = { last: Progress | null; chapters: Record<string, Progress> };

export type Health = { ok: boolean; name: string; version: string; suwayomi: boolean };

export type Me = { user: User; prefs: Record<string, unknown> };
