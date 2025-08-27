# PRD.md — legalpdf-to-md

## Judul & Ringkasan
Ekstraktor PDF regulasi menjadi Markdown legal-setia untuk RAG/QA hukum. Target utama: hasil MD yang konsisten, bisa diparse deterministik per unit hukum (BAB → Pasal → Ayat → Huruf), dengan jejak kualitas (meta.json) dan mode ketat (strict). Nilai: presisi kutip dan kesiapan ingest ke database/legal-AI tanpa mengorbankan kecepatan.

## Pengguna & Use-cases
- **Pengguna**: pengacara, mahasiswa hukum, peneliti kebijakan, paralegal, publik.
- **Skenario**:
  - Mengubah regulasi dari peraturan.bpk.go.id (born-digital) ke MD standar untuk di-index.
  - Menjalankan QA kualitas ekstraksi (coverage, struktur, kebocoran header/footer).
  - Menyiapkan bahan untuk pembuatan nodes (Pasal/Ayat/Huruf) dan chunk retrieval.

## Outcome & KPI
- **KPI-1 Struktur**: Akurasi deteksi heading **Pasal** (dan BAB) ≥ **98%** pada sampel.
- **KPI-2 Coverage**: **Character coverage** (MD bersih vs teks terambil) ≥ **99%**.
- **KPI-3 Leak Rate**: Header/footer & nomor halaman bocor = **0** pada output MD.
- **KPI-4 Latency**: p95 **≤ 400 ms/halaman** untuk born-digital (tanpa OCR) pada mesin kelas menengah; p95 OCR **≤ 5 s/halaman** @300dpi.

## Ruang Lingkup
- **Termasuk**:
  - Orkestrasi ekstraksi: **Poppler `pdftotext -layout`** (default), **PDFium** (opsional), **Tesseract OCR** (fallback per halaman “suspect”).
  - **Law-aware post-processing**: buang header/footer; join soft-wrap; hilangkan hyphenation; normalisasi list; promosikan Menimbang/Mengingat/BAB/Pasal/Penjelasan ke heading MD.
  - **Emit**: `out.md` **+** `out.meta.json` (coverage, halaman OCR, pola terdeteksi, exit codes).
  - **CLI flags**: `--engine`, `--with-ocr`, `--ocr-lang`, `--law-mode`, `--dump-steps`, `--strict`.
  - **Script instalasi Ubuntu** dengan **nala**: `poppler-utils`, `tesseract-ocr`, `tesseract-ocr-ind`, `pkg-config`, `clang`.
- **Tidak termasuk (Non-Goal)**:
  - Parser tabel kompleks dan layout multi-kolom tingkat lanjut (fase berikutnya).
  - Pembuatan **AST JSON** penuh (opsional fase 2).
  - Ingest ke database/embeddings (proyek terpisah).
  - Cross-reference dan versioning konsolidasi.

## Risiko & Trade-off
- **Scan/quality buruk** → OCR lambat/kurang akurat. *Mitigasi*: OCR per-halaman saja, log halaman rawan, izinkan re-run hanya halaman gagal.
- **Variasi redaksi antar instansi** → aturan regex perlu mode. *Mitigasi*: `--law-mode {auto|uu|pp|permen|perwali}`.
- **Multi-kolom/tabel** → Poppler bisa meleset. *Mitigasi*: gunakan `-layout`, heuristik join; sediakan **opsi PDFium** untuk fase lanjut.
- **Dependensi sistem** (Poppler/Tesseract) hilang. *Mitigasi*: **dependency check** awal + pesan perbaikan yang tegas.

## Timeline & Deliverables
- **M1 (minggu 1)**: Skeleton repo, CLI minimal, dependency check, jalur Poppler, `--dump-steps`.
- **M2 (minggu 2)**: Law-aware post-processor gelombang-1 (Menimbang/Mengingat/BAB/Pasal/Penjelasan), `meta.json`, `--strict`.
- **M3 (minggu 3)**: OCR fallback per halaman, pengukur coverage/leak/split-violations, snapshot tests 5–8 PDF.
- **M4 (minggu 4)**: Rilis **v0.1.0**: binary, README, contoh, workflow CI (fmt, clippy, tests).

**Artefak**: `legalpdf2md` (bin), `PRD.md`, `prd.yaml`, `README.md`, contoh `input/` & `output/`, `tests/snapshots/`, `ci.yml`.

## Pertanyaan Terbuka
- Apakah perlu **AST JSON** sebagai opsi output tambahan (fase 2)?
- Level agresivitas **heading detection** untuk case ambigu (ALL-CAPS panjang/pendek)?
- Batas **maks halaman** default sebelum auto-OCR dimatikan demi waktu?
