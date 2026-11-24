<script lang="ts">
  // La marca: tres lomos de libro (el del medio con el acento y dos bandas, el de la derecha
  // apoyado). Los tonos salen del tema, asi queda bien en claros y oscuros. Con `word`, el nombre
  // de brand.json al lado; con `shine`, un reflejo la cruza cada tanto (login, splash).
  import { brand } from "../lib/brand";

  let { size = 28, shine = false, word = false }: { size?: number; shine?: boolean; word?: boolean } = $props();
</script>

<span class="logo" class:shine style="--s: {size}px">
  <svg viewBox="0 0 320 340" width={size} height={size} aria-hidden="true">
    <rect class="back" x="20" y="40" width="64" height="276" rx="14" />
    <rect class="front" x="96" y="10" width="72" height="306" rx="14" />
    <rect class="band" x="96" y="60" width="72" height="14" />
    <rect class="band" x="96" y="256" width="72" height="14" />
    <rect class="lean" x="204" y="30" width="66" height="290" rx="14" transform="rotate(14 237 316)" />
  </svg>
  {#if word}<span class="word">{brand.name}</span>{/if}
</span>

<style>
  .logo {
    position: relative;
    display: inline-flex;
    align-items: center;
    gap: calc(var(--s) * 0.34);
    flex: none;
  }
  svg {
    display: block;
    overflow: visible;
  }
  .front {
    fill: var(--accent);
  }
  .band {
    fill: var(--accent-ink);
    opacity: 0.3;
  }
  .back {
    fill: color-mix(in srgb, var(--accent) 28%, var(--bg));
  }
  .lean {
    fill: color-mix(in srgb, var(--accent) 55%, var(--text));
  }
  .word {
    font-family: var(--font);
    font-weight: 720;
    font-size: calc(var(--s) * 0.66);
    letter-spacing: -0.025em;
    color: var(--text);
  }
  .shine::after {
    content: "";
    position: absolute;
    top: -10%;
    bottom: -10%;
    left: 0;
    width: calc(var(--s) * 0.5);
    background: linear-gradient(90deg, transparent, color-mix(in srgb, var(--text) 45%, transparent), transparent);
    mix-blend-mode: overlay;
    pointer-events: none;
    animation: glow-sweep 4.5s ease-in-out infinite;
  }
  .shine svg {
    filter: drop-shadow(0 0 calc(var(--s) * 0.18) color-mix(in srgb, var(--accent) 35%, transparent));
  }
</style>
