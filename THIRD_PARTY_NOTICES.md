# Third-party notices

The Windows build of Vouchify bundles the following third-party programs so the
PDF/OCR features work without a separate install. They are downloaded during the
release build (see `.github/workflows/release.yml`) and placed under
`resources/bundled/win/`.

## Poppler

- Provides: `pdftotext.exe`, `pdftoppm.exe` and supporting DLLs.
- Used for: extracting the layout-preserving text of Daily Entries reports, and
  rendering PDF pages to images for OCR.
- License: **GNU General Public License v2.0** (GPL-2.0).
- Source: https://poppler.freedesktop.org/ and the Windows build
  https://github.com/oschwartz10612/poppler-windows
- The full license text is included next to the binaries
  (`bundled/win/poppler/COPYING.txt`) when the release build can fetch it, and is
  available at https://www.gnu.org/licenses/old-licenses/gpl-2.0.html

## Tesseract OCR

- Provides: `tesseract.exe`, its DLLs and `tessdata` (eng, osd, chi_sim).
- Used for: OCR fallback for scanned/image-only PDFs.
- License: **Apache License 2.0**.
- Source: https://github.com/tesseract-ocr/tesseract
  (Windows installer from https://github.com/UB-Mannheim/tesseract/wiki)
- Language data is from https://github.com/tesseract-ocr/tessdata_fast

## ImageMagick (optional)

ImageMagick is **not** bundled. It is used only as an optional image
pre-processing step before OCR; when it is absent the raw page image is used
instead. If installed, it is detected automatically.
