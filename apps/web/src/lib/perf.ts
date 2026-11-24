// Equipos flojos (celulares viejos): se bajan los efectos caros sin cambiar el diseño. Con
// `data-lite` en <html> el CSS saca los desenfoques y la aparicion escalonada (base.css). Se puede
// forzar desde la consola: localStorage["library.lite"] = "1" (siempre) o "0" (nunca).
import { read } from "./store";

function detect(): boolean {
  const forced = read("library.lite");
  if (forced === "1") return true;
  if (forced === "0") return false;
  if (typeof navigator === "undefined") return false;
  const mem = (navigator as Navigator & { deviceMemory?: number }).deviceMemory;
  const cores = navigator.hardwareConcurrency || 8;
  return (mem !== undefined && mem <= 2) || cores <= 2;
}

export const lite = detect();

export function applyLite() {
  if (lite) document.documentElement.setAttribute("data-lite", "");
}
