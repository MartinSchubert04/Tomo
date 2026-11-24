<script lang="ts" generics="T">
  // Grilla de portadas con esqueletos mientras carga y, con `more`, scroll infinito: un centinela
  // al final pide la pagina siguiente cuando se acerca a la pantalla.
  import type { Snippet } from "svelte";

  let {
    items,
    card,
    loading = false,
    skeletons = 18,
    more,
    hasMore = false,
  }: {
    items: T[] | null;
    card: Snippet<[T, number]>;
    /** Cargando (la primera pagina si items es null, o la siguiente). */
    loading?: boolean;
    skeletons?: number;
    more?: () => void;
    hasMore?: boolean;
  } = $props();

  let sentinel = $state<HTMLDivElement>();
  // El observador se vuelve a crear cada vez que termina una carga: al observar avisa enseguida si
  // el centinela ya esta a la vista, asi en una pantalla alta se siguen pidiendo paginas hasta
  // llenarla (antes se quedaba en la segunda: el centinela nunca "entraba" de nuevo).
  $effect(() => {
    void items?.length;
    if (!sentinel || !more || !hasMore || loading) return;
    const io = new IntersectionObserver((es) => es.some((e) => e.isIntersecting) && more?.(), { rootMargin: "800px 0px" });
    io.observe(sentinel);
    return () => io.disconnect();
  });
</script>

<div class="cover-grid">
  {#if items}
    {#each items as it, i (i)}
      {@render card(it, i)}
    {/each}
  {/if}
  {#if loading || !items}
    {#each Array(items ? 6 : skeletons) as _, i (i)}
      <div class="ph" aria-hidden="true">
        <div class="skel frame"></div>
        <span class="skel line" style="width: {60 + ((i * 17) % 35)}%"></span>
      </div>
    {/each}
  {/if}
</div>
{#if hasMore && more}<div class="sentinel" bind:this={sentinel} aria-hidden="true"></div>{/if}

<style>
  .ph {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .frame {
    aspect-ratio: 2 / 3;
    border-radius: var(--radius);
  }
  .ph :global(.skel.line) {
    height: 10px;
  }
  .sentinel {
    height: 1px;
  }
</style>
