# DESIGN.md

Design system of the app (display name in `brand.json`). Format:
[awesome-design-md](https://github.com/VoltAgent/awesome-design-md) (nine sections). Ported from
Ethereon and Mythos: same themes, palettes, fonts, motion and base components (Select, Switch,
Seg, Confirm, Drawer, Section, toasts, tooltips), adapted to a manga and book reader. Read before
touching the interface (`apps/web`).

## 1. Visual theme and atmosphere

A reading app: covers rule everywhere except inside a reader, where the page is the only thing on
screen. It has to feel premium, quick and calm, like a good native app: every load has a shimmering
skeleton, every image fades in, nothing jumps, and the reader opens instantly on the page where you
left off.

- **References.** TuMangaOnline (cover grid with a type badge, chapter list with the scan group,
  "cascada" / "paginado" reader), Mihon/Tachiyomi (library with unread badges, global search grouped
  by source, reader settings sheet), Kavita/Komga (book shelf), Apple Books / Readest (book reader:
  clean page, settings in a sheet, progress at the bottom).
- **Density:** medium. Covers 120 to 170 px wide depending on the window (`auto-fill`), 2:3.
- **Voice:** literal, Spanish by default (neutral Rioplatense, voseo) or English. "Leer", "Seguir en
  el cap. 12", "Agregar a la biblioteca", "Subir libros", "Capítulo siguiente" / "Read", "Continue
  chapter 12", "Add to library", "Upload books", "Next chapter". No metaphors.
- **Six themes, one structure** (Default, Warp, Poolsuite FM, Haze, Athanor, Neovim, each with its
  palettes, exactly Mythos'). Colors, borders, shadows, backgrounds and fonts change; never the
  layout. Readers have their own page colors (section 4, "Reader") independent of the theme.

## 2. Colors and roles

All colors are variables in `apps/web/src/styles/themes.css` (same variables as Mythos). No
component has hardcoded colors.

| Variable | Role |
|---|---|
| `--bg`, `--bg-image` | Window background |
| `--sidebar`, `--panel`, `--panel-2` | Surfaces, from back to front |
| `--line`, `--line-strong`, `--border` | Borders and their width |
| `--text`, `--text-2`, `--text-3` | Primary, secondary and tertiary text |
| `--accent`, `--accent-ink`, `--primary-bg` | Accent, text on the accent, primary button. Also progress bars and unread badges |
| `--danger`, `--warn`, `--ok` | Delete or error; NSFW / mixed source; done, installed, up to date |
| `--media-bg`, `--on-media`, `--on-media-2`, `--media-scrim`, `--media-line` | Reader chrome over the page (bars, sheets), same in every theme |
| `--page-bg`, `--page-fg` | The reader's page colors, set by the reader settings, not by the theme |

Palettes: `apps/web/scripts/palettes.py` generates `styles/palettes.css` and `lib/palettes.gen.ts`
(`npm run palettes`); each theme remembers its own.

## 3. Typography

Inter Variable for the interface, Geist Mono Variable for numbers that change (page 12/48, %,
sizes) with `tabular-nums`. Bundled with `@fontsource` (no network).

Exceptions as in Mythos: Athanor (PxPlus IBM VGA 8x16 at 16 px, titles in Jacquard 24) and Neovim
(JetBrains Mono). **Book text** uses the reader's font setting: Literata (serif, bundled), Inter
(sans) or the book's own fonts ("Original").

| Use | Size | Weight |
|---|---|---|
| Detail title (`.display-title`) | 26 to 44 px (clamp) | 760 |
| Page title | 21 px | 650 |
| Row title | 16 px | 650 |
| Card title | 13 px, two lines max | 560 |
| Text | 13.5 px (descriptions 14.5 px) | 400 to 560 |
| Section labels | 11.5 px uppercase | 650 |

## 4. Components

- **Cover card** (`CoverCard`): 2:3 image, `loading="lazy"`, fades in; title below in two lines.
  Badges on the image: unread count (accent pill, top right), format for books (`EPUB`, `PDF`,
  `CBZ`, top left), a progress bar at the bottom edge if started. Hover: rises 4 px with a spring,
  accent border and glow, a sheen crosses it. Long press / right click: menu (add to library, mark
  read, remove). Without an image: typographic cover (accent gradient + title).
- **Row** (`Row`): title (+ arrow link to the full page), horizontal scroll with arrows on hover,
  loads its data when it nears the screen, skeletons meanwhile.
- **Continue card** (`ContinueCard`): landscape card with the cover blurred as background, the cover
  small on the left, title, "Cap. 12 · pág. 8/40" or "45 %", progress bar. Tap = straight into the
  reader at that spot.
- **Source chip** (`SourceChip`): icon + name + lang; NSFW sources show a `--warn` dot.
- **Chapter list** (`ChapterList`): rows of 52 px with number/name, scan group and relative date;
  read ones dimmed with a check, the current one with a progress ring. Windowed (only rows near the
  viewport are rendered: lists reach 1500 chapters). Sort ↑/↓, filter by scan group, "mark
  previous as read" from the row menu, select mode for bulk read/unread.
- **Manga detail** (`views/Manga.svelte`): blurred cover as a backdrop fading into `--bg`; cover,
  title, author, status chip, genres as chips, description clamped to 4 lines ("Más"), primary
  button "Leer" / "Seguir en el cap. N", library button with status menu, heart for favorite,
  "Abrir en el sitio" link. On the phone the cover goes above, centered.
- **Manga reader** (`views/Reader.svelte`): see "Reader".
- **Book reader** (`views/BookReader.svelte`): foliate-js `<foliate-view>` for EPUB, MOBI, AZW3, FB2;
  pdf.js for PDF (pages as canvases in the same cascade/paged engine as manga); CBZ uses the manga
  reader with `/api/books/{id}/pages`. Settings sheet: font (Original, Literata, Inter), size,
  line height, margins, justify, paged / scroll, page color. Table of contents in a drawer. Bottom:
  slider with % and chapter name.
- **Upload dropzone** (`Dropzone`): the whole Books view accepts dropped files (an accent dashed
  overlay appears: "Soltá para subir"); also a button. Each upload shows a card with a progress bar
  (XHR upload progress) that turns into the real cover when done.
- **Extension row** (Admin → Fuentes): icon, name, lang, version, NSFW badge, Install / Update /
  Uninstall button with a spinner; language filter (Seg: Español / Todos / Instaladas) and search.
- **Select, Switch, Seg, Confirm, Drawer, Section, PopMenu, toasts, tooltips**: Mythos'.

### Reader

The page is the protagonist: black (or the chosen page color) edge to edge, chrome hidden.

- **Modes**: *Cascada* (vertical strip, default for manhwa/webtoon and on the phone), *Paginado*
  (one page, swipe or tap sides), *Doble* (two pages side by side on wide screens). Direction for
  paged: right-to-left (manga) or left-to-right. Fit: width, height, original. In cascade: max
  width (600 to 1400 px, default 900) and gap between pages (0 / 8 / 24 px).
- **Chrome** (`--media-*`): top bar (back, manga title, chapter name, chapter picker, settings)
  and bottom bar (prev chapter, page slider with "12 / 40", next chapter). Shown on tap in the
  center third or when scrolling up; hidden while scrolling down and after 3 s idle.
- **Tap zones** (paged): left third / right third turn pages (mirrored for RTL), center toggles the
  chrome. Keyboard: ←/→, A/D, space, PgUp/PgDn, Home/End, `[`/`]` previous/next chapter, F
  fullscreen, M mode, Esc back.
- **Loading**: every page reserves its height (aspect ratio from the first loaded page, then each
  page's own) so the strip never jumps; a spinner and "Reintentar" on failed pages. Preload the
  next 3 pages, and the next chapter's page list at 80 %.
- **End of chapter**: a card "Fin del capítulo N" with the next chapter button; in cascade, keep
  scrolling and the next chapter appends (infinite reading, like TMO's cascade).
- **Progress**: saved every 3 s while reading and on leaving; a chapter counts as read at the last
  page.

## 4b. States

| State | What's shown |
|---|---|
| Loading | Shimmering skeletons in grids, rows, detail and chapter list; splash with the brand on open |
| Empty | Icon + sentence + action: empty library ("Explorá"), no books ("Subí tus libros"), search with no results, no sources installed (admins: "Instalá fuentes" → Admin; others: "Pedile al admin que instale fuentes") |
| Source failing | In global search, the source's group shows the error in `--text-3` and "Reintentar"; in browse, an error card with "Reintentar" |
| Manga server down | Banner "El servidor de manga no responde" in manga views; books keep working |
| Server down | Pill at the top "No se puede conectar con el servidor. Reintentando…"; retries every 5 s |
| Session expired (401) | Login with "Tu sesión venció" |

## 5. Layout

**Desktop (≥ 760 px):** sidebar on the left (220 px, or 64 px collapsed with only icons;
remembered) with Inicio, Explorar, Biblioteca, Libros, Historial, and at the bottom Ajustes, Admin
(admins only) and the user. Top bar inside the content with the global search (`/` or `Ctrl K`).
The content scrolls in its own container and remembers where it was when coming back.

**Phone (< 760 px):** bottom tab bar (Inicio, Explorar, Biblioteca, Libros, Más) with
`env(safe-area-inset-bottom)`; the search is a button in the top bar that opens a full-screen
search. Touch targets ≥ 44 px. Installable PWA (standalone, theme color from `brand.json`).

Readers take the whole screen (no sidebar, no tab bar).

Hash routes: `#/`, `#/explore`, `#/explore/{sourceId}?type=&q=`, `#/search?q=`, `#/library`,
`#/books`, `#/history`, `#/manga/{workId}`, `#/read/{workId}/{chapterId}?page=`,
`#/book/{workId}`, `#/settings`, `#/admin`. Each view is loaded only when needed.

## 5b. Motion

All animations go through `apps/web/src/lib/motion.ts` or are short CSS from `base.css`, and last 0
with the system's or Settings' "reduce motion". **Only `transform` and `opacity`.**

| What | How |
|---|---|
| Changing view | rises 6 px and appears (220 ms) |
| Card → detail | the cover flies to the detail cover (View Transitions, 380 ms) |
| Cards | appear staggered (30 ms between each); hover rises with a spring |
| Images | 400 ms fade when they finish loading |
| Loading | skeletons with a sheen (1.4 s) |
| Drawer / sheets | from the right (desktop) or from the bottom (phone) with a bit of spring |
| Dialogs, menus | grow from 95 % (160 ms) |
| Buttons | rise 1 px on hover, sink when pressed; the primary has a sheen |
| Reader chrome | bars slide out to their edge (200 ms) |
| Page turn (paged) | horizontal slide following the finger, snaps with a spring |
| Toasts | come in from the bottom and rearrange (flip) |

In Athanor motion is stepped (`steps()`).

## 6. Depth

As Mythos: no shadows at rest in Default, Warp, Haze and Neovim; on hover covers get a shadow and
an accent glow. Poolsuite: hard offset shadows. Athanor: one step up, no soft shadow. Floating
things use `--pop-shadow`. Reader chrome uses dark glass.

## 7. Dos and don'ts

- Do: never a blank screen. Always a skeleton or the splash.
- Do: reserve image sizes (`aspect-ratio`) so nothing jumps.
- Do: states with color and text at once ("NSFW" chip in `--warn`, "Instalada" in `--ok`).
- Do: every action reachable with a finger on a 390 px screen.
- Don't: hardcoded colors or visible strings in components.
- Don't: animate layout properties, blur in animations, or long animations (> 400 ms) except the
  view transition.

## 8. Responsive behavior

Breakpoint 760 px (sidebar ↔ tab bar). Cover grid `repeat(auto-fill, minmax(120px, 1fr))` on the
phone, `minmax(150px, 1fr)` on desktop. Detail: two columns ≥ 900 px. Reader: cascade max width
from the settings; paged fits the viewport; double page only ≥ 1000 px wide and landscape.

## 9. Agent guide

Before changing the interface: read this file and `styles/themes.css`. Try it in at least Default,
Poolsuite (light, 2 px borders) and Athanor (bitmap font), at 1440 px and 390 px.
`npm run check` with no warnings.
