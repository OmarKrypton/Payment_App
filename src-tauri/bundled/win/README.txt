This folder holds the PDF/OCR tools bundled with the Windows build.

The Windows release workflow (.github/workflows/release.yml) populates it with:

  win/poppler/    pdftotext.exe, pdftoppm.exe and their DLLs
  win/tesseract/  tesseract.exe, its DLLs, and tessdata (eng, osd, chi_sim)

Nothing else is needed here at build time; when the folder is empty the app
simply falls back to tools installed on the machine. The actual binaries are
.gitignored and never committed.
