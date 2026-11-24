// PDF con pdf.js: cada pagina es un PageItem que se dibuja en un canvas al ancho que tenga en
// pantalla (nitido con la densidad de pixeles del equipo). El archivo se pide por rangos (el
// server soporta Range), asi un PDF de 300 MB abre sin bajarlo entero.
import * as pdfjs from "pdfjs-dist";
import workerUrl from "pdfjs-dist/build/pdf.worker.min.mjs?url";
import type { PageItem } from "./pages";

pdfjs.GlobalWorkerOptions.workerSrc = workerUrl;

export type PdfBook = {
  pages: PageItem[];
  count: number;
  /** La primera pagina como imagen (para la portada). */
  cover: () => Promise<Blob | null>;
  destroy: () => void;
};

export async function openPdf(url: string): Promise<PdfBook> {
  const task = pdfjs.getDocument({ url, disableAutoFetch: true, disableStream: false, rangeChunkSize: 1 << 18 });
  const doc = await task.promise;
  const first = await doc.getPage(1);
  const v1 = first.getViewport({ scale: 1 });
  const ratio = v1.width / v1.height;
  const dpr = Math.min(2.5, window.devicePixelRatio || 1);

  const pages: PageItem[] = Array.from({ length: doc.numPages }, (_, i) => ({
    ratio,
    draw: async (canvas: HTMLCanvasElement, cssWidth: number) => {
      const page = await doc.getPage(i + 1);
      const base = page.getViewport({ scale: 1 });
      const scale = (cssWidth * dpr) / base.width;
      const vp = page.getViewport({ scale });
      canvas.width = Math.floor(vp.width);
      canvas.height = Math.floor(vp.height);
      await page.render({ canvas, viewport: vp }).promise;
      return base.width / base.height;
    },
  }));

  async function cover(): Promise<Blob | null> {
    const vp = first.getViewport({ scale: 400 / v1.width });
    const canvas = document.createElement("canvas");
    canvas.width = Math.floor(vp.width);
    canvas.height = Math.floor(vp.height);
    await first.render({ canvas, viewport: vp }).promise;
    return new Promise((resolve) => canvas.toBlob((b) => resolve(b), "image/webp", 0.85));
  }

  return { pages, count: doc.numPages, cover, destroy: () => void task.destroy() };
}
