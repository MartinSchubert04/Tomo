// Todos los textos en inglés (mismas claves que ../es).
import account from "./account";
import admin from "./admin";
import books from "./books";
import browse from "./browse";
import common from "./common";
import reader from "./reader";

export default { ...common, ...browse, ...reader, ...books, ...account, ...admin };
