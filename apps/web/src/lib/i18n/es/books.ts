// Textos en español: books. La estanteria y las subidas.
import type { Msg } from "../types";

export default {
  "books.count": { one: "{n} libro", other: "{n} libros" },
  "books.upload": "Subir libros",
  "books.all": "Todos",
  "books.other": "Otros",
  "books.filter": "Filtrar por título o autor",
  "books.empty": "Subí tus libros",
  "books.emptyHint": "Tocá acá o soltá archivos en cualquier parte. Se comparten con todos los usuarios del servidor.",
  "books.noMatch": "Ningún libro coincide",
  "books.dropHere": "Soltá para subir",
  "books.unsupported": { one: "{n} archivo no es un formato soportado", other: "{n} archivos no son de un formato soportado" },
  "books.edit": "Editar datos",
  "books.titleField": "Título",
  "books.author": "Autor",
  "books.uploadedBy": "subido por {name}",
  "books.deleteTitle": "¿Borrar “{title}”?",
  "books.deleteBody": "Se borra el archivo del servidor para todos los usuarios.",
  "books.deleted": "Libro borrado",
} satisfies Record<string, Msg>;
