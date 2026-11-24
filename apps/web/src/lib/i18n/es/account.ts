// Textos en español: account. Login y ajustes.
import type { Msg } from "../types";

export default {
  "login.user": "Usuario",
  "login.password": "Contraseña",
  "login.submit": "Entrar",
  "login.wrong": "Usuario o contraseña incorrectos",
  "login.tooMany": "Demasiados intentos. Esperá unos minutos",
  "login.expired": "Tu sesión venció. Entrá de nuevo.",
  "login.hint": "Las cuentas las crea el admin del servidor.",

  "settings.look": "Apariencia",
  "settings.theme": "Tema",
  "settings.paletteFor": "Paleta de {theme}",
  "settings.reduceMotion": "Reducir animaciones",
  "settings.reading": "Lectura",
  "settings.readingHint": "Lo que usa el lector al abrir. Se guarda en este dispositivo (en el celu podés leer en cascada y en la PC paginado).",
  "settings.bookFlow": "Libros: pasar páginas",
  "settings.sources": "Fuentes",
  "settings.sourcesHint": "Las fijadas van primero en Explorar, el inicio usa la primera y la búsqueda global busca en ellas.",
  "settings.pinnedN": { one: "{n} fijada", other: "{n} fijadas" },
  "settings.nsfw": "Mostrar fuentes para adultos",
  "settings.account": "Cuenta",
  "settings.changePassword": "Cambiar la contraseña",
  "settings.currentPassword": "Contraseña actual",
  "settings.newPassword": "Contraseña nueva",
  "settings.savePassword": "Cambiar contraseña",
  "settings.passwordShort": "La contraseña tiene que tener al menos 6 caracteres",
  "settings.passwordChanged": "Contraseña cambiada",
} satisfies Record<string, Msg>;
