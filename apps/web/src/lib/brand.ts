// El nombre de la app y sus textos de marca. Viven solo en brand.json (raiz del repo): para
// renombrar la app se cambia ese archivo y nada mas.
import brandJson from "../../../../brand.json";
import { i18n } from "./i18n.svelte";

export const brand = brandJson as { name: string; tagline: string; taglineEn: string; themeColor: string };

/** El lema en el idioma de la interfaz. */
export const tagline = () => (i18n.lang === "en" ? brand.taglineEn : brand.tagline);
