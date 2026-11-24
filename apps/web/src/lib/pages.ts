// Lo que muestra el lector de paginas (manga, CBZ y PDF): una lista de "segmentos" (capitulos)
// con sus paginas. Una pagina es una imagen (URL) o algo que se dibuja en un canvas (PDF).

export type PageItem =
  | { src: string; ratio?: number }
  | {
      /** Dibuja la pagina en el canvas al ancho dado (px CSS). Devuelve ancho / alto. */
      draw: (canvas: HTMLCanvasElement, cssWidth: number) => Promise<number>;
      ratio?: number;
    };

export type Segment = {
  id: string;
  title: string;
  pages: PageItem[];
};

/** Relacion ancho / alto que se reserva antes de saber la real (una pagina de manga). */
export const DEFAULT_RATIO = 0.7;
