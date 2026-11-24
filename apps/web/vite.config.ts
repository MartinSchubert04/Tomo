import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { svelte } from "@sveltejs/vite-plugin-svelte";
import { defineConfig, type Plugin } from "vite";

// La interfaz. En desarrollo /api va al server local (cd backend; cargo run -p library).
// Con --mode mock (VITE_MOCK=1) no hace falta server: ver src/lib/mock/.

/** El nombre de la app vive solo en brand.json (raiz del repo). */
const brandPath = fileURLToPath(new URL("../../brand.json", import.meta.url));
const brand = JSON.parse(readFileSync(brandPath, "utf8")) as { name: string; tagline: string; themeColor: string };

/** Pone el nombre y el color en index.html (%BRAND_NAME%, %BRAND_COLOR%, %BRAND_TAGLINE%) y genera
 *  el manifest de la PWA con los mismos datos. */
function brandPlugin(): Plugin {
  const manifest = () =>
    JSON.stringify({
      name: brand.name,
      short_name: brand.name,
      description: brand.tagline,
      start_url: "./",
      scope: "./",
      display: "standalone",
      background_color: brand.themeColor,
      theme_color: brand.themeColor,
      icons: [
        { src: "icon.svg", sizes: "any", type: "image/svg+xml", purpose: "any" },
        { src: "icon-192.png", sizes: "192x192", type: "image/png", purpose: "any" },
        { src: "icon-512.png", sizes: "512x512", type: "image/png", purpose: "any" },
        { src: "maskable-512.png", sizes: "512x512", type: "image/png", purpose: "maskable" },
        { src: "icon-maskable.svg", sizes: "any", type: "image/svg+xml", purpose: "maskable" },
      ],
    });
  return {
    name: "library-brand",
    transformIndexHtml(html) {
      return html
        .replaceAll("%BRAND_NAME%", brand.name)
        .replaceAll("%BRAND_COLOR%", brand.themeColor)
        .replaceAll("%BRAND_TAGLINE%", brand.tagline);
    },
    configureServer(server) {
      server.middlewares.use("/manifest.webmanifest", (_req, res) => {
        res.setHeader("Content-Type", "application/manifest+json");
        res.end(manifest());
      });
    },
    generateBundle() {
      this.emitFile({ type: "asset", fileName: "manifest.webmanifest", source: manifest() });
    },
  };
}

/** Service worker: guarda el "cascaron" de la app (index.html y los assets con hash) para que
 *  abra al instante y sin red. Nunca /api. La lista de assets se escribe en el build. */
function swPlugin(): Plugin {
  return {
    name: "library-sw",
    apply: "build",
    generateBundle(_o, bundle) {
      // Solo JS y CSS: las fuentes (un archivo por alfabeto) y lo pesado (pdf.js) se guardan
      // recien cuando se usan, desde el fetch del service worker.
      const files = Object.keys(bundle).filter((f) => f.startsWith("assets/") && /\.(js|css)$/.test(f) && !/pdf/.test(f));
      const version = Date.now().toString(36);
      const src = readFileSync(fileURLToPath(new URL("./src/sw.js", import.meta.url)), "utf8")
        .replace("__VERSION__", version)
        .replace("__ASSETS__", JSON.stringify(["./", ...files]));
      this.emitFile({ type: "asset", fileName: "sw.js", source: src });
    },
  };
}

export default defineConfig({
  plugins: [svelte(), brandPlugin(), swPlugin()],
  clearScreen: false,
  server: {
    port: 5173,
    strictPort: true,
    fs: { allow: [".", brandPath] },
    proxy: { "/api": { target: "http://127.0.0.1:7710", changeOrigin: false } },
  },
  preview: { port: 5173, proxy: { "/api": "http://127.0.0.1:7710" } },
  build: {
    target: ["es2022", "chrome111", "safari16"],
    modulePreload: { polyfill: false },
    chunkSizeWarningLimit: 1200,
  },
});
