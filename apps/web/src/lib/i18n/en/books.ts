// English texts: books (mismas claves que ../es/books.ts).
import type es from "../es/books";
import type { Shape } from "../types";

export default {
  "books.count": { one: "{n} book", other: "{n} books" },
  "books.upload": "Upload books",
  "books.all": "All",
  "books.other": "Other",
  "books.filter": "Filter by title or author",
  "books.empty": "Upload your books",
  "books.emptyHint": "Tap here or drop files anywhere. They're shared with every user on the server.",
  "books.noMatch": "No book matches",
  "books.dropHere": "Drop to upload",
  "books.unsupported": { one: "{n} file isn't a supported format", other: "{n} files aren't a supported format" },
  "books.edit": "Edit details",
  "books.titleField": "Title",
  "books.author": "Author",
  "books.uploadedBy": "uploaded by {name}",
  "books.deleteTitle": "Delete “{title}”?",
  "books.deleteBody": "The file is deleted from the server for every user.",
  "books.deleted": "Book deleted",
} satisfies Shape<typeof es>;
