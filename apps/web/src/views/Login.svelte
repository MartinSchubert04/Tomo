<script lang="ts">
  // Login: centrado, con la marca que brilla y manchas del acento moviendose atras. Contraseña
  // incorrecta: la tarjeta tiembla.
  import { LoaderCircle } from "@lucide/svelte";
  import Logo from "../components/Logo.svelte";
  import { ApiError, errText } from "../lib/api";
  import { brand, tagline } from "../lib/brand";
  import { t } from "../lib/i18n.svelte";
  import { login, session } from "../lib/session.svelte";

  let username = $state("");
  let password = $state("");
  let error = $state("");
  let busy = $state(false);
  let shake = $state(false);
  let userField = $state<HTMLInputElement>();

  $effect(() => {
    if (!matchMedia("(pointer: coarse)").matches) userField?.focus();
  });

  async function submit(e: Event) {
    e.preventDefault();
    if (!username.trim() || !password) return;
    error = "";
    busy = true;
    try {
      await login(username.trim(), password);
    } catch (err) {
      if (err instanceof ApiError && err.status === 401) error = t("login.wrong");
      else if (err instanceof ApiError && err.status === 429) error = t("login.tooMany");
      else error = errText(err);
      password = "";
      shake = false;
      requestAnimationFrame(() => (shake = true));
    } finally {
      busy = false;
    }
  }
</script>

<div class="wrap">
  <div class="aurora" aria-hidden="true"><span></span><span></span><span></span></div>
  <div class="shaker" class:shake>
    <form class="box card" onsubmit={submit}>
      <div class="mark"><Logo size={58} shine /></div>
      <h1>{brand.name}</h1>
      <p class="muted">{tagline()}</p>
      {#if session.expired}<div class="notice">{t("login.expired")}</div>{/if}
      <label class="field">
        {t("login.user")}
        <input bind:this={userField} bind:value={username} autocomplete="username" autocapitalize="off" spellcheck="false" />
      </label>
      <label class="field">
        {t("login.password")}
        <input type="password" bind:value={password} autocomplete="current-password" />
      </label>
      {#if error}<div class="err">{error}</div>{/if}
      <button class="btn primary big" disabled={busy || !username.trim() || !password}>
        {#if busy}<LoaderCircle size={16} class="spin" />{/if}
        {t("login.submit")}
      </button>
      <div class="faint small">{t("login.hint")}</div>
    </form>
  </div>
</div>

<style>
  .wrap {
    position: relative;
    min-height: var(--vh);
    display: grid;
    place-items: center;
    padding: 20px;
    overflow: hidden;
  }
  .aurora {
    position: absolute;
    inset: 0;
    pointer-events: none;
    opacity: 0.6;
  }
  .aurora span {
    position: absolute;
    width: 42vmax;
    height: 42vmax;
    border-radius: 50%;
    background: radial-gradient(circle, color-mix(in srgb, var(--accent) 22%, transparent), transparent 62%);
    animation: drift 18s ease-in-out infinite alternate;
    will-change: transform;
  }
  .aurora span:nth-child(1) {
    left: -10vmax;
    top: -14vmax;
  }
  .aurora span:nth-child(2) {
    right: -14vmax;
    bottom: -18vmax;
    animation-duration: 23s;
    animation-delay: -6s;
    background: radial-gradient(circle, color-mix(in srgb, var(--accent) 18%, var(--text) 6%), transparent 65%);
  }
  .aurora span:nth-child(3) {
    left: 40%;
    top: 55%;
    width: 26vmax;
    height: 26vmax;
    animation-duration: 29s;
    animation-delay: -12s;
  }
  @keyframes drift {
    to {
      transform: translate(8vmax, 6vmax) scale(1.15);
    }
  }
  :global([data-theme="poolsuite"]) .aurora,
  :global([data-theme="athanor"]) .aurora {
    display: none;
  }
  .shaker {
    position: relative;
    width: min(380px, 100%);
  }
  .box {
    width: 100%;
    padding: 30px 28px 24px;
    display: flex;
    flex-direction: column;
    gap: 13px;
    box-shadow: var(--pop-shadow);
    background: var(--sidebar);
    animation: rise 0.45s cubic-bezier(0.2, 0.8, 0.2, 1) backwards;
  }
  :global([data-theme="warp"]) .box,
  :global([data-theme="haze"]) .box {
    background: color-mix(in srgb, var(--bg) 70%, transparent);
    backdrop-filter: blur(24px);
  }
  .mark {
    display: grid;
    place-items: center;
    margin: 2px 0 4px;
  }
  h1 {
    margin: 0;
    font-family: var(--display);
    font-size: 24px;
    font-weight: 750;
    letter-spacing: -0.03em;
    text-align: center;
  }
  p {
    margin: -8px 0 6px;
    text-align: center;
    font-size: 13px;
  }
  .notice {
    padding: 8px 10px;
    border-radius: var(--radius-sm);
    color: var(--text);
    background: color-mix(in srgb, var(--warn) 14%, transparent);
    border: var(--border) solid color-mix(in srgb, var(--warn) 40%, transparent);
  }
  input {
    height: 42px;
    font-size: 15px;
  }
  .big {
    height: 44px;
    margin-top: 4px;
    font-size: 14.5px;
  }
  .err {
    color: var(--danger);
    font-size: 12.5px;
  }
  .small {
    font-size: 11.5px;
    text-align: center;
  }
</style>
