// Iconos en PNG/ICO a partir de public/icon.svg y public/icon-maskable.svg (iOS no usa SVG para
// el apple-touch-icon y algunos navegadores piden PNG en el manifest). Correr despues de cambiar
// los SVG:  npm run icons   (deja los archivos en public/, que se commitean).
import { readFileSync, writeFileSync } from "node:fs";
import sharp from "sharp";

const pub = new URL("../public/", import.meta.url);
const svg = readFileSync(new URL("icon.svg", pub));
const maskable = readFileSync(new URL("icon-maskable.svg", pub));

const png = (src, size) => sharp(src, { density: 384 }).resize(size, size).png({ compressionLevel: 9 }).toBuffer();

const out = {
  "icon-192.png": await png(svg, 192),
  "icon-512.png": await png(svg, 512),
  "maskable-512.png": await png(maskable, 512),
  // iOS redondea solo: el fondo lleno (el maskable, sin esquinas transparentes).
  "apple-touch-icon.png": await png(maskable, 180),
};
for (const [name, buf] of Object.entries(out)) writeFileSync(new URL(name, pub), buf);

// favicon.ico con PNG de 32 y 48 adentro (el formato ICO acepta PNG tal cual).
const sizes = [32, 48];
const imgs = await Promise.all(sizes.map((s) => png(svg, s)));
const head = Buffer.alloc(6 + 16 * imgs.length);
head.writeUInt16LE(0, 0);
head.writeUInt16LE(1, 2);
head.writeUInt16LE(imgs.length, 4);
let offset = head.length;
imgs.forEach((img, i) => {
  const e = 6 + 16 * i;
  head.writeUInt8(sizes[i], e);
  head.writeUInt8(sizes[i], e + 1);
  head.writeUInt8(0, e + 2);
  head.writeUInt8(0, e + 3);
  head.writeUInt16LE(1, e + 4);
  head.writeUInt16LE(32, e + 6);
  head.writeUInt32LE(img.length, e + 8);
  head.writeUInt32LE(offset, e + 12);
  offset += img.length;
});
writeFileSync(new URL("favicon.ico", pub), Buffer.concat([head, ...imgs]));
console.log("iconos listos:", [...Object.keys(out), "favicon.ico"].join(", "));
