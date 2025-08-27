# legalpdf-to-md

> Legal PDF → Markdown yang deterministik, **taat struktur hukum**, dengan OCR terukur dan acceptance checks ketat.

## Judul & Deskripsi Singkat

**legalpdf-to-md** mengubah PDF regulasi (lahir‑digital maupun hasil scan) menjadi Markdown yang bersih dan konsisten untuk RAG/QA hukum. Pipeline ini menjaga heading hukum (BAB, Pasal, Menimbang/Mengingat, PENJELASAN) sekaligus menekan noise seperti header/footer dan nomor halaman.

## Badge

[![GitHub](https://img.shields.io/github/license/yourusername/legalpdf-to-md)](https://github.com/rfxlamia/legalpdf-to-md/blob/main/LICENSE)
[![Rust](https://img.shields.io/badge/Rust-CLI-000?logo=rust&logoColor=white)](https://www.rust-lang.org/)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)

## Daftar Isi

* [Fitur Utama](#fitur-utama)
* [Instalasi](#instalasi)
* [Cara Pakai (Usage)](#cara-pakai-usage)
* [Konfigurasi](#konfigurasi)
* [Arsitektur/Struktur Repo](#arsitekturstruktur-repo)
* [Roadmap / Status Proyek](#roadmap--status-proyek)
* [Kontribusi](#kontribusi)
* [Lisensi](#lisensi)
* [Kontak / Kredit](#kontak--kredit)

## Fitur Utama

* **Ekstraksi Poppler**: per‑halaman via `pdftotext` (+ `pdfinfo` jika tersedia) dengan `-layout` dan kontrol pemisahan halaman.
* **Deteksi halaman “suspect”**: heuristik *low‑text* → halaman kandidat OCR.
* **OCR deterministik (Minor‑Patch‑III)**: `pdftoppm` → `tesseract` per halaman "suspect" (default `-l ind`, PSM=4, OEM=1) + fallback adaptif (`ind+eng`/PSM=6 bila kosong). Artefak tersimpan opsional di `artifacts/ocr/page-{n}.png`.
* **Suppressor repeated‑line** lintas halaman dengan whitelist regex (opsional) untuk menekan kebocoran header/footer periodik.
* **Law‑aware cleanup**: buang header/footer & nomor halaman, perbaiki hyphenasi dan soft‑wrap.
* **Promosi heading hukum** → Markdown deterministik: `## BAB …`, `## Pasal N`, `## Menimbang`, `## Mengingat`, `## PENJELASAN`, subjudul penjelasan `### I./II.`.
* **Emisi output atomik**: `<doc_id>.md` + `<doc_id>.meta.json` per dokumen; berisi fingerprint, metrik (coverage karakter, leak rate, p95 latency/halaman), statistik cleanup, serta ringkasan OCR.
* **Acceptance runner** (`scripts/acceptance.sh`): cek skema meta, akurasi struktur vs *ground truth*, tidak ada kebocoran artefak sementara, dan **idempotensi** meta.

## Instalasi

> Dites pada Ubuntu 24.04 LTS.

### Dependensi sistem (via **nala**)

```bash
sudo nala install poppler-utils tesseract-ocr tesseract-ocr-ind pkg-config clang jq ripgrep
```

### Toolchain Rust

* Disarankan: **rustup** (terbaru)

  ```bash
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
  rustup default stable
  ```
* Alternatif cepat: `sudo nala install cargo` (versi repo bisa lebih lama).

## Cara Pakai (Usage)

1. **Siapkan struktur input minimal**

   ```text
   ./input/uu/...
   ./input/pp/...
   ./input/permen/...
   ```

2. **Buat/cek `prd.yaml` minimal**

   ```yaml
   version: 1
   id: proj-legalpdf-to-md
   tools:
     - name: check_deps
     - name: enumerate_pdfs
   datasources:
     - path: "./input/**/*.pdf"
   outputs:
     dir: "./output"
   ```

3. **Jalankan pipeline**

   ```bash
   cargo run --release -- \
     --with-ocr=on \
     --ocr-lang ind \
     --ocr-dpi 300 \
     --per-doc-dir=on \
     --artifacts=off
   ```

   Output default:

   ```text
   output/<doc_id>/
   ├─ <doc_id>.md
   ├─ <doc_id>.meta.json
   └─ artifacts/                 # hanya jika --artifacts=on atau --dump-steps
      ├─ step1_extract.txt
      ├─ step2_merge.txt
      ├─ suppressor_preview.txt
      ├─ step3_md.txt
      └─ ocr/page-1.png, page-2.png, ...
   ```

4. **Acceptance (opsional tapi disarankan)**

   ```bash
   bash scripts/acceptance.sh --artifacts=off --idempotency-fast --ci-sample 3 --ocr-dpi 300
   # Laporan ringkas
   cat output/accept_table.txt
   ```

5. **Bekukan ground truth struktur** (setelah hasil stabil)

   ```bash
   bash scripts/gen_ground_truth.sh
   git add tests/fixtures/ground_truth.yaml && git commit -m "freeze GT"
   ```

## Konfigurasi

### Flag CLI

| Flag            | Nilai                    | Default                                                           | Fungsi                                                  |
| --------------- | ------------------------ | ----------------------------------------------------------------- | ------------------------------------------------------- |
| `--with-ocr`    | `on`\|`off`              | *auto*: `on` bila ada halaman "suspect" **dan** deps OCR tersedia | Memaksa nyalakan/matikan OCR.                           |
| `--ocr-lang`    | contoh: `ind`, `ind+eng` | `ind`                                                             | Bahasa OCR Tesseract.                                   |
| `--ocr-dpi`     | angka (≥72)              | `300`                                                             | DPI render `pdftoppm` sebelum OCR.                      |
| `--law-mode`    | `auto` (saat ini)        | `auto`                                                            | Mode heuristik hukum.                                   |
| `--keep-lines`  | regex                    | *(none)*                                                          | Whitelist baris agar tidak disuppress.                  |
| `--dump-steps`  | (tanpa nilai)            | *off*                                                             | Tulis step preview ke `artifacts/` untuk debug.         |
| `--artifacts`   | `on`\|`off`              | `off`                                                             | Simpan artefak dan preview langkah.                     |
| `--per-doc-dir` | `on`\|`off`              | `on`                                                              | Struktur `output/<doc_id>/...` per dokumen.             |
| `--strict`      | (tanpa nilai)            | *off*                                                             | Keluar non‑zero pada pelanggaran serius (struktur/OCR). |

### Variabel lingkungan

| Variabel             | Contoh | Efek                                                                                                |
| -------------------- | ------ | --------------------------------------------------------------------------------------------------- |
| `CI_SAMPLE_SUSPECTS` | `3`    | Batasi OCR hanya pada N halaman "suspect" pertama (mempercepat CI). Digunakan oleh `acceptance.sh`. |

## Arsitektur/Struktur Repo

```text
legalpdf-to-md/
├─ src/
│  ├─ lib.rs          # inti: check_deps, enumerate_pdfs, poppler_extract, suppress_repeated_lines,
│  │                  # ocr_tesseract, merge_pages, law_cleanup, promote_legal_headings, compute_metrics, emit_files
│  └─ main.rs         # CLI: parsing flag, orkestrasi, meta & emisi, idempotensi
├─ scripts/
│  ├─ acceptance.sh   # acceptance: skema meta, OCR coverage, ground truth, idempotensi
│  └─ gen_ground_truth.sh
├─ tests/
│  ├─ check_deps_tests.rs
│  ├─ enumerate_pdfs_tests.rs
│  ├─ poppler_detect_tests.rs
│  ├─ merge_pages_tests.rs
│  ├─ law_cleanup_promote_tests.rs
│  ├─ compute_emit_tests.rs
│  └─ fixtures/
│     └─ ground_truth.yaml
├─ prd.yaml           # spesifikasi mesin (datasource glob, output dir, tools minimal)
├─ prd.md             # PRD naratif (KPI: struktur≥98%, coverage≥99%, leak=0, p95≤400ms/hal.)
└─ patch/
   ├─ minor-patch
   ├─ minor-patch-ii
   ├─ minor-patch-iii
   └─ refinement-patch
```

**Skema meta (ringkas)**

```json
{
  "doc_id": "…",
  "engine": "poppler",
  "suspect_pages": [..],
  "ocr": {
    "enabled": true,
    "ran": true,
    "skipped_reason": null,
    "ocr_run_pages": [..],
    "lang": "ind",
    "psm": 4,
    "oem": 1,
    "dpi": 300
  },
  "found": {"bab": 11, "pasal": 164, "menimbang": true, "mengingat": true, "penjelasan": true},
  "stats": {"removed_header": 1, "removed_footer": 1, "hyphens_fixed": 3},
  "metrics": {"character_coverage": 0.992, "leak_rate": 0.0, "split_violations": 0, "coverage_pages": 1.0},
  "page_count": 200,
  "timing_ms_per_page": [..],
  "p95_latency_ms_per_page": 320,
  "timestamps": {"started_ms": 0, "finished_ms": 0}
}
```

## Roadmap / Status Proyek

Status: **beta stabil** untuk dokumen lahir‑digital; **robust** untuk image‑scan setelah *Minor‑Patch‑III*.

* [x] Poppler extract & per‑page orchestration
* [x] Deteksi halaman "suspect" + OCR deterministik & artefak
* [x] Law‑aware cleanup + heading promotion
* [x] Metrik (coverage, leak, split violations) & meta fingerprint
* [x] Acceptance: idempotensi, ground truth, no‑step‑leak
* [ ] Parser tabel kompleks & multi‑kolom
* [ ] Mode *engine* alternatif (mis. pdfminer) jika Poppler bermasalah
* [ ] Benchmark suite & profil p95 per keluarga dokumen

## Kontribusi

1. **Diskusi isu**: jelaskan tipe PDF (lahir‑digital vs scan), contoh halaman bermasalah, dan log `--dump-steps`.
2. **PR**: sertakan test minimal (`tests/*`) + update `ground_truth.yaml` bila berdampak ke struktur.
3. **Gaya**: idiomatik Rust 2021, tanpa `unwrap()` di jalur utama, error eksplisit.
4. **CI**: jalankan `bash scripts/acceptance.sh` lokal sebelum mengirim PR.

## Lisensi

MIT License

## Kontak / Kredit

Maintainer: **Rafi** · Legal‑tech & AI

Dokumen terkait: `prd.md`, patch notes (`patch/*`).
