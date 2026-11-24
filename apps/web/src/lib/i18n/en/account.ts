// English texts: account (mismas claves que ../es/account.ts).
import type es from "../es/account";
import type { Shape } from "../types";

export default {
  "login.user": "User name",
  "login.password": "Password",
  "login.submit": "Sign in",
  "login.wrong": "Wrong user name or password",
  "login.tooMany": "Too many attempts. Wait a few minutes",
  "login.expired": "Your session expired. Sign in again.",
  "login.hint": "Accounts are created by the server's admin.",

  "settings.look": "Appearance",
  "settings.theme": "Theme",
  "settings.paletteFor": "{theme} palette",
  "settings.reduceMotion": "Reduce motion",
  "settings.reading": "Reading",
  "settings.readingHint": "What the reader uses when it opens. Saved on this device (read in cascade on your phone and paged on the PC).",
  "settings.bookFlow": "Books: turn pages",
  "settings.sources": "Sources",
  "settings.sourcesHint": "Pinned sources go first in Explore, Home uses the first one and global search searches them.",
  "settings.pinnedN": { one: "{n} pinned", other: "{n} pinned" },
  "settings.nsfw": "Show adult sources",
  "settings.account": "Account",
  "settings.changePassword": "Change password",
  "settings.currentPassword": "Current password",
  "settings.newPassword": "New password",
  "settings.savePassword": "Change password",
  "settings.passwordShort": "The password needs at least 6 characters",
  "settings.passwordChanged": "Password changed",
} satisfies Shape<typeof es>;
