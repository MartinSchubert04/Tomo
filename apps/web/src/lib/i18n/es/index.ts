// Todos los textos en español. El español es la fuente de las claves (lib/i18n.svelte.ts).
import account from "./account";
import admin from "./admin";
import books from "./books";
import browse from "./browse";
import common from "./common";
import reader from "./reader";

export default { ...common, ...browse, ...reader, ...books, ...account, ...admin };
