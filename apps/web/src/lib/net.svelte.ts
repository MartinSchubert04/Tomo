// Server caido: cualquier pedido que no llega lo marca y App muestra la pastilla de arriba
// ("No se puede conectar con el servidor. Reintentando…"). Se reintenta /api/health cada 5 s y,
// al volver, App recarga la vista (net.back cambia).
export const net = $state({ down: false, back: 0 });

let timer = 0;

export function markDown() {
  if (net.down) return;
  net.down = true;
  clearInterval(timer);
  timer = window.setInterval(async () => {
    try {
      const r = await fetch("/api/health", { cache: "no-store" });
      if (r.ok) markUp();
    } catch {}
  }, 5000);
}

export function markUp() {
  if (!net.down) return;
  net.down = false;
  net.back++;
  clearInterval(timer);
}
