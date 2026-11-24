/// <reference types="svelte" />
/// <reference types="vite/client" />

interface ImportMetaEnv {
  readonly VITE_MOCK?: string;
}

// foliate-js (src/vendor/foliate-js) es JS sin tipos.
declare module "*/foliate-js/view.js";
