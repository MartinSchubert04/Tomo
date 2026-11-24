# foliate-js (vendored)

E-book renderer used by the book reader for EPUB, MOBI, AZW3 and FB2.

- Upstream: https://github.com/johnfactotum/foliate-js (MIT, see `LICENSE`)
- Commit: `78914aef4466eb960965702401634c2cb348e9b1` (2026-05-01)
- Files: only what `view.js` loads for those formats (`view.js`, `epub.js`, `epubcfi.js`,
  `mobi.js`, `fb2.js`, `comic-book.js`, `paginator.js`, `fixed-layout.js`, `progress.js`,
  `overlayer.js`, `text-walker.js`, `search.js`, `tts.js`, `vendor/zip.js`, `vendor/fflate.js`).
- Local change: `view.js` no longer opens PDFs (that branch imported upstream's vendored pdf.js);
  PDFs go through `pdfjs-dist` in the app's page reader. Search for "(Library)" in `view.js`.

To update: copy the same files from a newer commit, re-apply the change and update the commit above.
