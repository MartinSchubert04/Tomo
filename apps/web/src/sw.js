// Service worker: el cascaron de la app (index.html + assets con hash) sale de la cache, asi abre
// al instante y aunque el server tarde. /api nunca pasa por aca (datos siempre frescos).
// vite.config.ts (swPlugin) pone la version y la lista de assets en el build.
const CACHE = "shell-__VERSION__";
const ASSETS = __ASSETS__;

self.addEventListener("install", (e) => {
  e.waitUntil(
    caches
      .open(CACHE)
      .then((c) => c.addAll(ASSETS))
      .then(() => self.skipWaiting()),
  );
});

self.addEventListener("activate", (e) => {
  e.waitUntil(
    caches
      .keys()
      .then((keys) => Promise.all(keys.filter((k) => k !== CACHE).map((k) => caches.delete(k))))
      .then(() => self.clients.claim()),
  );
});

self.addEventListener("fetch", (e) => {
  const req = e.request;
  if (req.method !== "GET") return;
  const url = new URL(req.url);
  if (url.origin !== location.origin || url.pathname.startsWith("/api/")) return;
  // Navegacion: la red primero (asi una version nueva se ve enseguida), la cache si no hay red.
  if (req.mode === "navigate") {
    e.respondWith(fetch(req).catch(() => caches.match("./")));
    return;
  }
  // Assets con hash: no cambian nunca, la cache primero.
  if (url.pathname.includes("/assets/")) {
    e.respondWith(
      caches.match(req).then(
        (hit) =>
          hit ||
          fetch(req).then((res) => {
            if (res.ok) {
              const copy = res.clone();
              caches.open(CACHE).then((c) => c.put(req, copy));
            }
            return res;
          }),
      ),
    );
  }
});
