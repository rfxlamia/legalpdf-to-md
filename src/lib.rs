use std::path::{Path, PathBuf};
use std::process::Command;

use globwalk::GlobWalkerBuilder;
use regex::Regex;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DepsResult {
    pub ok: bool,
    pub missing: Vec<String>,
}

/// Check required/optional CLI dependencies.
/// - Required: pdftotext (Poppler)
/// - Optional: tesseract (OCR)
/// Returns a DepsResult. `ok` is true iff required deps are present.
pub fn check_deps() -> DepsResult {
    let mut missing = Vec::new();

    // required
    let has_pdftotext = which::which("pdftotext").is_ok();
    if !has_pdftotext {
        missing.push("pdftotext".to_string());
    }
    // required for OCR image rendering
    let has_pdftoppm = which::which("pdftoppm").is_ok();
    if !has_pdftoppm {
        missing.push("pdftoppm".to_string());
    }

    // optional
    if which::which("tesseract").is_err() {
        missing.push("tesseract".to_string());
    }

    DepsResult { ok: has_pdftotext && has_pdftoppm, missing }
}

#[derive(Debug, Error)]
pub enum EnumerateError {
    #[error("NoFilesFound")]
    NoFilesFound { guidance: String },
}

/// Enumerate PDFs using a glob pattern (e.g., "./input/**/*.pdf").
/// Returns a sorted list of paths.
pub fn enumerate_pdfs(glob_pattern: &str) -> Result<Vec<PathBuf>, EnumerateError> {
    let root = if Path::new(glob_pattern).is_absolute() { "/" } else { "." };
    let mut pat = glob_pattern.to_string();
    if pat.starts_with("./") { pat = pat.trim_start_matches("./").to_string(); }
    let mut paths: Vec<PathBuf> = GlobWalkerBuilder::from_patterns(root, &[pat.as_str()])
        .case_insensitive(false)
        .follow_links(false)
        .max_depth(std::usize::MAX)
        .build()
        .map_err(|_| EnumerateError::NoFilesFound { guidance: folder_guidance() })?
        .filter_map(|e| e.ok())
        .map(|e| e.path().to_path_buf())
        .collect();

    paths.sort();
    paths.retain(|p| p.is_file());

    if paths.is_empty() {
        return Err(EnumerateError::NoFilesFound { guidance: folder_guidance() });
    }

    Ok(paths)
}

fn folder_guidance() -> String {
    // Keep concise, actionable guide per PRD
    let guide = r#"Tidak ada PDF pada pola ./input/**/*.pdf
Struktur yang disarankan:
  ./input/uu/...
  ./input/pp/...
  ./input/permen/...
  ./input/perwali/...
Contoh: letakkan berkas PDF di ./input/uu/NOMOR-TAHUN.pdf"#;
    guide.to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrdRoot {
    pub id: String,
    #[serde(default)]
    pub tools: Option<Vec<PrdTool>>,
    #[serde(default)]
    pub datasources: Option<Vec<PrdDatasource>>, // supports new schema
    #[serde(default)]
    pub outputs: Option<PrdOutputs>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrdTool {
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrdDatasource {
    pub name: Option<String>,
    pub path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrdOutputs {
    pub dir: Option<String>,
    #[serde(default)]
    pub artifacts_dir: Option<String>,
}

#[derive(Debug, Error)]
pub enum PrdError {
    #[error("Failed to read prd.yaml: {0}")]
    Read(String),
    #[error("Failed to parse prd.yaml: {0}")]
    Parse(String),
    #[error("Invalid PRD: {0}")]
    Invalid(String),
}

/// Minimal validation for prd.yaml according to provided spec.
pub fn validate_prd(prd_path: &Path) -> Result<PrdRoot, PrdError> {
    let raw = std::fs::read_to_string(prd_path).map_err(|e| PrdError::Read(e.to_string()))?;
    let prd: PrdRoot = serde_yaml::from_str(&raw).map_err(|e| PrdError::Parse(e.to_string()))?;

    if prd.id.trim().is_empty() {
        return Err(PrdError::Invalid("missing id".into()));
    }

    // Accept either legacy file_patterns (not present in current PRD) or new schema (datasources + outputs)
    let has_ds_glob = prd
        .datasources
        .as_ref()
        .and_then(|ds| ds.first())
        .and_then(|d| d.path.clone())
        .is_some();
    let has_out_dir = prd.outputs.as_ref().and_then(|o| o.dir.clone()).is_some();
    if !has_ds_glob || !has_out_dir {
        // Don't fail hard; mark invalid for visibility
        return Err(PrdError::Invalid("missing datasources.path or outputs.dir".into()));
    }

    // Ensure tools contain check_deps and enumerate_pdfs
    let tools = prd.tools.clone().unwrap_or_default();
    let names: Vec<String> = tools.into_iter().map(|t| t.name).collect();
    for required in ["check_deps", "enumerate_pdfs"] {
        if !names.iter().any(|n| n == required) {
            return Err(PrdError::Invalid(format!("missing tool: {}", required)));
        }
    }

    Ok(prd)
}

impl PrdRoot {
    pub fn input_glob(&self) -> String {
        self.datasources
            .as_ref()
            .and_then(|d| d.first())
            .and_then(|d| d.path.clone())
            .unwrap_or_else(|| "./input/**/*.pdf".to_string())
    }
    pub fn output_dir(&self) -> String {
        self.outputs
            .as_ref()
            .and_then(|o| o.dir.clone())
            .unwrap_or_else(|| "./output".to_string())
    }
}

/// Render Nala installation help for missing deps.
pub fn nala_help_for(missing: &[String]) -> String {
    let mut pkgs: Vec<&str> = Vec::new();
    if missing.iter().any(|m| m == "pdftotext") {
        pkgs.push("poppler-utils");
        // pkg-config + clang are in PRD, include when core dep missing
        pkgs.push("pkg-config");
        pkgs.push("clang");
    }
    if missing.iter().any(|m| m == "tesseract") {
        pkgs.push("tesseract-ocr");
        pkgs.push("tesseract-ocr-ind");
    }

    if pkgs.is_empty() {
        return String::new();
    }

    format!(
        "Dependency missing. Install via Nala:\n  sudo nala install {}",
        pkgs.join(" ")
    )
}

#[derive(Debug, Error)]
pub enum PopplerError {
    #[error("FileNotFound: {0}")]
    FileNotFound(String),
    #[error("EncryptedPDF: {0}")]
    EncryptedPDF(String),
    #[error("PopplerError: {0}")]
    Other(String),
}

/// Extract text pages using Poppler's pdftotext.
/// Prefers per-page extraction with -layout -nopgbrk when pdfinfo is available for page count.
/// Falls back to single pass without -nopgbrk and split on form feed when pdfinfo is missing.
pub fn poppler_extract(path: &Path, layout: bool, nopgbrk: bool) -> Result<Vec<String>, PopplerError> {
    if !path.exists() {
        return Err(PopplerError::FileNotFound(path.display().to_string()));
    }

    let use_pdfinfo = which::which("pdfinfo").is_ok();
    let pages_count = if use_pdfinfo {
        match Command::new("pdfinfo").arg(path).output() {
            Ok(out) => {
                if !out.status.success() {
                    let err = String::from_utf8_lossy(&out.stderr).to_lowercase();
                    if err.contains("encrypt") || err.contains("password") {
                        return Err(PopplerError::EncryptedPDF(path.display().to_string()));
                    }
                    None
                } else {
                    let s = String::from_utf8_lossy(&out.stdout);
                    let mut pages: Option<usize> = None;
                    for line in s.lines() {
                        if let Some(rest) = line.strip_prefix("Pages:") {
                            pages = rest.trim().parse::<usize>().ok();
                            break;
                        }
                    }
                    pages
                }
            }
            Err(_) => None,
        }
    } else {
        None
    };

    if let Some(n_pages) = pages_count {
        // Per-page extraction using -f i -l i
        let mut pages: Vec<String> = Vec::with_capacity(n_pages);
        for i in 1..=n_pages {
            let mut cmd = Command::new("pdftotext");
            if layout {
                cmd.arg("-layout");
            }
            if nopgbrk {
                cmd.arg("-nopgbrk");
            }
            cmd.arg("-q");
            cmd.arg("-f").arg(i.to_string());
            cmd.arg("-l").arg(i.to_string());
            cmd.arg(path);
            cmd.arg("-"); // write to stdout

            let out = cmd.output().map_err(|e| PopplerError::Other(e.to_string()))?;
            if !out.status.success() {
                let err = String::from_utf8_lossy(&out.stderr).to_lowercase();
                if err.contains("encrypt") || err.contains("password") {
                    return Err(PopplerError::EncryptedPDF(path.display().to_string()));
                }
                return Err(PopplerError::Other(format!("pdftotext failed on page {}", i)));
            }
            let text = String::from_utf8_lossy(&out.stdout).to_string();
            pages.push(text);
        }
        Ok(pages)
    } else {
        // Fallback: single pass, split by form feed (\x0c), do not use -nopgbrk so page breaks exist
        let mut cmd = Command::new("pdftotext");
        if layout {
            cmd.arg("-layout");
        }
        // Intentionally not adding -nopgbrk so we can split by page breaks
        cmd.arg("-q");
        cmd.arg(path);
        cmd.arg("-");
        let out = cmd.output().map_err(|e| PopplerError::Other(e.to_string()))?;
        if !out.status.success() {
            let err = String::from_utf8_lossy(&out.stderr).to_lowercase();
            if err.contains("encrypt") || err.contains("password") {
                return Err(PopplerError::EncryptedPDF(path.display().to_string()));
            }
            return Err(PopplerError::Other("pdftotext failed".into()));
        }
        let s = String::from_utf8_lossy(&out.stdout);
        let mut pages: Vec<String> = s.split('\u{000C}').map(|x| x.to_string()).collect();
        // drop trailing empty page if any
        while matches!(pages.last(), Some(last) if last.trim().is_empty()) {
            pages.pop();
        }
        Ok(pages)
    }
}

/// Return 0-based indices of pages whose non-whitespace characters are less than min_chars.
pub fn detect_suspect_pages(pages: &[String], min_chars: usize) -> Vec<usize> {
    let mut out = Vec::new();
    for (idx, page) in pages.iter().enumerate() {
        let count = page.chars().filter(|c| !c.is_whitespace()).count();
        if count < min_chars {
            out.push(idx);
        }
    }
    out
}

#[derive(Debug, Clone)]
pub struct SuppressorConfig {
    pub threshold_ratio: f64,               // e.g., 0.60
    pub keep_lines: Option<Regex>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SuppressorStats {
    pub removed_lines_sample: Vec<String>,
    pub suppressor_overrun: usize,
    pub removed_header: usize,
    pub removed_footer: usize,
}

/// Suppress repeated headers/footers and page numbers conservatively before cleanup.
/// Returns new pages and stats.
pub fn suppress_repeated_lines(pages: &[String], cfg: &SuppressorConfig) -> (Vec<String>, SuppressorStats, Vec<String>) {
    let page_count = pages.len().max(1);
    let threshold = ((cfg.threshold_ratio * page_count as f64).ceil() as usize).max(1);

    let re_num_dash = Regex::new(r"(?m)^\s*[\u2012\u2013\u2014\u2212\-]{1,3}\s*\d+\s*[\u2012\u2013\u2014\u2212\-]{1,3}\s*$").unwrap();
    let re_hal = Regex::new(r"(?mi)^\s*(Hal(?:\.|aman))\s*\d+\s*$").unwrap();
    let re_plain_num = Regex::new(r"(?m)^\s*\d{1,4}\s*$").unwrap();
    let re_head1 = Regex::new(r"(?mi)^\s*PRESIDEN\s+REPUBLIK\s+INDONESIA\s*$").unwrap();
    let re_head2 = Regex::new(r"(?mi)^\s*KEMENTERIAN\s+KETENAGAKERJAAN\s*(RI)?\s*$").unwrap();
    let re_head3 = Regex::new(r"(?mi)^\s*(TAMBAHAN\s+)?LEMBARAN\s+NEGARA\s+REPUBLIK\s+INDONESIA.*$").unwrap();
    let re_whitelist = Regex::new(r"(?i)^(BAB\s+[IVXLCDM]|Pasal\s+\d+|Menimbang:?|Mengingat:?|PENJELASAN)\b").unwrap();

    use std::collections::HashMap;
    let mut freq: HashMap<String, usize> = HashMap::new();
    let mut top: HashMap<String, usize> = HashMap::new();
    let mut bottom: HashMap<String, usize> = HashMap::new();

    for (pi, page) in pages.iter().enumerate() {
        let lines: Vec<&str> = page.lines().collect();
        for (li, raw) in lines.iter().enumerate() {
            let mut line = raw.trim();
            if line.is_empty() { continue; }
            if re_whitelist.is_match(line) { continue; }
            // Normalize spaces
            let norm = Regex::new(r"\s+").unwrap().replace_all(line, " ").to_string();
            *freq.entry(norm.clone()).or_insert(0) += 1;
            if li == 0 { *top.entry(norm.clone()).or_insert(0) += 1; }
            if li + 1 == lines.len() { *bottom.entry(norm.clone()).or_insert(0) += 1; }
        }
    }

    let mut to_remove_repeated: HashMap<String, ()> = HashMap::new();
    for (line, &c) in freq.iter() {
        if c >= threshold {
            let len = line.len();
            if len >= 3 && len <= 120 && !re_whitelist.is_match(line) {
                let t = *top.get(line).unwrap_or(&0);
                let b = *bottom.get(line).unwrap_or(&0);
                if t * 2 >= c || b * 2 >= c { // position heuristic
                    to_remove_repeated.insert(line.clone(), ());
                }
            }
        }
    }

    let mut stats = SuppressorStats::default();
    let mut removed_samples: Vec<String> = Vec::new();
    let mut new_pages: Vec<String> = Vec::with_capacity(pages.len());

    for page in pages.iter() {
        let mut removed_this_page = 0usize;
        let mut kept: Vec<String> = Vec::new();
        for raw in page.lines() {
            let line = raw.trim_end();
            let mut drop = false;
            // strong patterns
            if re_head1.is_match(line) || re_head2.is_match(line) || re_head3.is_match(line) {
                drop = true; stats.removed_header += 1;
            } else if re_num_dash.is_match(line) || re_hal.is_match(line) {
                drop = true; stats.removed_footer += 1;
            } else if re_plain_num.is_match(line) {
                // only if frequent and appears in repeated list
                let norm = Regex::new(r"\s+").unwrap().replace_all(line.trim(), " ").to_string();
                if to_remove_repeated.contains_key(&norm) { drop = true; stats.removed_footer += 1; }
            } else {
                let norm = Regex::new(r"\s+").unwrap().replace_all(line.trim(), " ").to_string();
                if to_remove_repeated.contains_key(&norm) { drop = true; }
            }
            if drop {
                if let Some(re) = &cfg.keep_lines {
                    if re.is_match(line) { drop = false; }
                }
            }
            if drop {
                removed_this_page += 1;
                if removed_samples.len() < 5 {
                    removed_samples.push(line.trim().to_string());
                }
                if removed_this_page > 5 {
                    stats.suppressor_overrun += 1;
                    kept.push(line.to_string()); // stop dropping too many; keep rest
                }
                continue;
            }
            kept.push(line.to_string());
        }
        new_pages.push(kept.join("\n"));
    }

    stats.removed_lines_sample = removed_samples;
    (new_pages, stats, to_remove_repeated.keys().cloned().collect())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OcrText {
    pub index: usize,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OcrErrorEntry {
    pub index: usize,
    pub message: String,
}

#[derive(Debug, Clone)]
pub struct OcrOutcome {
    pub texts: Vec<OcrText>,
    pub failed: Vec<usize>,
    pub skipped_due_to_missing_deps: bool,
    pub errors: Vec<OcrErrorEntry>,
}

/// Optional OCR for suspect pages using `pdftoppm` and `tesseract`.
/// - pages: 0-based indices to OCR
/// - Returns texts for successfully OCR-ed pages, and failed indices.
/// - Never panics; if deps are missing, marks skipped and returns no texts.
pub fn ocr_tesseract(path: &Path, pages: &[usize], lang: &str, dpi: u32, artifacts_dir: Option<&Path>, psm: u8, oem: u8) -> OcrOutcome {
    let has_pdftoppm = which::which("pdftoppm").is_ok();
    let has_tesseract = which::which("tesseract").is_ok();
    if !has_pdftoppm || !has_tesseract {
        return OcrOutcome { texts: vec![], failed: pages.to_vec(), skipped_due_to_missing_deps: true, errors: vec![] };
    }
    let tmpdir = tempfile::tempdir().ok();

    let mut texts = Vec::new();
    let mut failed = Vec::new();
    let mut errors = Vec::new();

    for &idx0 in pages {
        let page_no = (idx0 + 1) as i32; // pdftoppm is 1-based
        // Always render into temp path, then copy into artifacts/ocr if requested
        let base = tmpdir.as_ref().map(|d| d.path().to_path_buf()).unwrap_or_else(|| std::env::temp_dir());
        let render_prefix = base.join(format!("p{}", page_no));
        let render_img = render_prefix.with_extension("png");
        let artifact_img = artifacts_dir.map(|ad| {
            let ocr_dir = ad.join("ocr");
            let _ = std::fs::create_dir_all(&ocr_dir);
            ocr_dir.join(format!("page-{}.png", page_no))
        });

        // Render page to PNG via pdftoppm
        let out = Command::new("pdftoppm")
            .arg("-r").arg(dpi.to_string())
            .arg("-f").arg(page_no.to_string())
            .arg("-l").arg(page_no.to_string())
            .arg("-png")
            .arg("-singlefile")
            .arg(path)
            .arg(&render_prefix)
            .output();
        match out {
            Ok(o) if o.status.success() => {}
            _ => { failed.push(idx0); errors.push(OcrErrorEntry{ index: idx0, message: "pdftoppm_failed".into()}); continue; }
        }
        // Verify image exists and size > 0
        if !render_img.exists() {
            failed.push(idx0);
            errors.push(OcrErrorEntry{ index: idx0, message: "image_missing".into()});
            continue;
        }
        if let Ok(meta) = std::fs::metadata(&render_img) {
            if meta.len() == 0 { failed.push(idx0); errors.push(OcrErrorEntry{ index: idx0, message: "image_zero_size".into()}); continue; }
        }

        // Tesseract OCR to stdout
        let mut run_tess = |lang_arg: &str, psm_arg: u8, oem_arg: u8| -> Result<String, String> {
            let out = Command::new("tesseract")
                .arg(&render_img)
                .arg("stdout")
                .arg("-l").arg(lang_arg)
                .arg("--psm").arg(psm_arg.to_string())
                .arg("--oem").arg(oem_arg.to_string())
                .output();
            match out {
                Ok(o) if o.status.success() => {
                    let s = String::from_utf8_lossy(&o.stdout).to_string();
                    if s.trim().is_empty() { Err("empty_text".into()) } else { Ok(s) }
                }
                Ok(o) => Err(format!("tesseract_exit_{}", o.status.code().unwrap_or(-1))),
                Err(e) => Err(format!("tesseract_spawn_error: {}", e)),
            }
        };

        // primary attempt
        match run_tess(lang, psm, oem) {
            Ok(text) => {
                texts.push(OcrText { index: idx0, text });
            }
            Err(e1) => {
                // fallback once: try lang ind+eng keeping psm/oem; if still empty/error, try psm=6
                let fallback_lang = if lang.contains('+') { lang } else { "ind+eng" };
                match run_tess(fallback_lang, psm, oem) {
                    Ok(text) => { texts.push(OcrText { index: idx0, text }); }
                    Err(e2) => {
                        // final attempt with psm=6
                        match run_tess(fallback_lang, 6, oem) {
                            Ok(text) => { texts.push(OcrText { index: idx0, text }); }
                            Err(e3) => { failed.push(idx0); errors.push(OcrErrorEntry{ index: idx0, message: format!("{};{};{}", e1, e2, e3)}); }
                        }
                    }
                }
            }
        }

        // If artifacts dir is requested and render succeeded (not failed), copy image for traceability
        if let Some(dst) = artifact_img.as_ref() {
            if !failed.contains(&idx0) {
                let _ = std::fs::copy(&render_img, dst);
            }
        }
    }

    OcrOutcome { texts, failed, skipped_due_to_missing_deps: false, errors }
}

/// Merge pages with OCR overrides. Overrides replace the corresponding page text by index.
pub fn merge_pages(pages: &[String], overrides: &[OcrText]) -> String {
    let mut out_pages: Vec<String> = pages.to_vec();
    for ov in overrides {
        if let Some(slot) = out_pages.get_mut(ov.index) {
            *slot = ov.text.clone();
        }
    }
    out_pages.join("\n")
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CleanupStats {
    pub removed_header: usize,
    pub removed_footer: usize,
    pub hyphens_fixed: usize,
    #[serde(default)]
    pub removed_lines_sample: Vec<String>,
    #[serde(default)]
    pub suppressor_overrun: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CleanupOutput {
    pub cleaned: String,
    pub stats: CleanupStats,
}

/// Minimal, safe law-aware cleanup.
pub fn law_cleanup(text: &str, _law_mode: &str) -> CleanupOutput {
    // 1) Remove hyphenation across lines: (\w)-\n(\w) -> $1$2
    let hyphen_re = Regex::new(r"(\w)-\n(\w)").unwrap();
    let hyphens_fixed = hyphen_re.find_iter(text).count();
    let no_hyph = hyphen_re.replace_all(text, "$1$2").into_owned();

    // 2) Remove common header/footer lines
    let header_re = Regex::new(r"(?mi)^\s*PRESIDEN\s+REPUBLIK\s+INDONESIA\s*$").unwrap();
    let header2_re = Regex::new(r"(?mi)^\s*KEMENTERIAN\s+KETENAGAKERJAAN\s*(RI)?\s*$").unwrap();
    let header3_re = Regex::new(r"(?mi)^\s*(TAMBAHAN\s+)?LEMBARAN\s+NEGARA\s+REPUBLIK\s+INDONESIA.*$").unwrap();
    let footer_re = Regex::new(r"(?m)^\s*-\s*\d+\s*-\s*$").unwrap();
    let footer_dash_re = Regex::new(r"(?m)^\s*[\u2012\u2013\u2014\u2212\-]{1,3}\s*\d+\s*[\u2012\u2013\u2014\u2212\-]{1,3}\s*$").unwrap();
    let footer_hal_re = Regex::new(r"(?mi)^\s*(Hal(?:\.|aman))\s*\d+\s*$").unwrap();
    let footer_plainnum_re = Regex::new(r"(?m)^\s*\d{1,3}\s*$").unwrap();
    let mut removed_header = 0usize;
    let mut removed_footer = 0usize;
    let mut kept_lines: Vec<String> = Vec::new();
    for line in no_hyph.lines() {
        if header_re.is_match(line) || header2_re.is_match(line) || header3_re.is_match(line) {
            removed_header += 1;
            continue;
        }
        if footer_re.is_match(line) || footer_dash_re.is_match(line) || footer_hal_re.is_match(line) || footer_plainnum_re.is_match(line) {
            removed_footer += 1;
            continue;
        }
        kept_lines.push(line.to_string());
    }

    // 3) Join soft-wrap: line ending with alnum continues with a space
    let mut joined = String::new();
    let mut prev_ended_alnum = false;
    for (i, line) in kept_lines.iter().enumerate() {
        let trimmed_next = if i > 0 && prev_ended_alnum { line.trim_start() } else { line.as_str() };
        if i > 0 {
            if prev_ended_alnum && !joined.ends_with(':') && !joined.ends_with(';') {
                joined.push(' ');
            } else {
                joined.push('\n');
            }
        }
        joined.push_str(trimmed_next);
        // treat heading lines as non-alnum enders
        let is_heading = Regex::new(r"^(?i)(BAB\s+[IVXLCDM]|Pasal\s+\d+|Menimbang:?|Mengingat:?|PENJELASAN)\b").unwrap();
        prev_ended_alnum = !is_heading.is_match(line)
            && line.chars().rev().find(|c| !c.is_whitespace()).map(|c| c.is_ascii_alphanumeric()).unwrap_or(false);
    }

    // 4) Normalize lists
    let letter_re = Regex::new(r"^\s*([a-z])\.\s+").unwrap();
    let num_re = Regex::new(r"^\s*\d+\.\s+").unwrap();
    let orphan_paren = Regex::new(r"(?m)^\s*\((\d+)\)\s*$").unwrap();
    let orphan_num = Regex::new(r"(?m)^\s*([0-9]+)\.\s*$").unwrap();
    let orphan_letter = Regex::new(r"(?m)^\s*([a-z])\.\s*$").unwrap();
    let mut out_lines = Vec::new();
    let lines: Vec<String> = joined.lines().map(|s| s.to_string()).collect();
    let mut i = 0;
    while i < lines.len() {
        let line = &lines[i];
        let mut merged_line = line.clone();
        let mut consumed_next = false;
        if (orphan_paren.is_match(line) || orphan_num.is_match(line) || orphan_letter.is_match(line)) && i + 1 < lines.len() {
            let next = &lines[i + 1];
            let is_heading_next = Regex::new(r"^(?i)(BAB\s+[IVXLCDM]|Pasal\s+\d+|Menimbang:?|Mengingat:?|PENJELASAN)\b").unwrap();
            if !next.trim().is_empty() && !is_heading_next.is_match(next) {
                let token = if let Some(c) = orphan_paren.captures(line) { format!("({})", &c[1]) }
                    else if let Some(c) = orphan_num.captures(line) { format!("{}.", &c[1]) }
                    else if let Some(c) = orphan_letter.captures(line) { format!("{}.", &c[1]) } else { String::new() };
                merged_line = format!("{} {}", token, next.trim_start());
                consumed_next = true;
            }
        }
        let norm = if let Some(c) = letter_re.captures(&merged_line) {
            letter_re.replace(&merged_line, format!("- ({} )", &c[1]).as_str()).into_owned()
        } else if num_re.is_match(&merged_line) {
            num_re.replace(&merged_line, "1. ").into_owned()
        } else {
            merged_line
        };
        out_lines.push(norm);
        i += 1 + if consumed_next { 1 } else { 0 };
    }
    let cleaned = out_lines.join("\n");

    CleanupOutput {
        cleaned,
        stats: CleanupStats { removed_header, removed_footer, hyphens_fixed, removed_lines_sample: Vec::new(), suppressor_overrun: 0 },
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Found {
    pub pasal: usize,
    pub bab: usize,
    pub menimbang: bool,
    pub mengingat: bool,
    pub penjelasan: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromoteOutput {
    pub markdown: String,
    pub found: Found,
}

/// Promote legal headings to Markdown according to minimal patterns.
pub fn promote_legal_headings(input: &str, _law_mode: &str) -> PromoteOutput {
    // Prepare regexes per-line
    let re_mm = Regex::new(r"^\s*(Menimbang|Mengingat)\s*:\s*$").unwrap();
    let re_bab = Regex::new(r"^\s*BAB\s+([IVXLCDM]+)\b(.*)$").unwrap();
    let re_pasal = Regex::new(r"^\s*Pasal\s+(\d+)\s*$").unwrap();
    let re_penj = Regex::new(r"^\s*PENJELASAN\s*$").unwrap();
    let re_rom_sub = Regex::new(r"^\s*([IVX]+)\.\s+([A-Z][^\n]+)$").unwrap();

    let mut out = Vec::new();
    let mut found = Found::default();
    for line in input.lines() {
        if let Some(cap) = re_mm.captures(line) {
            let title = cap.get(1).unwrap().as_str();
            if title.eq_ignore_ascii_case("Menimbang") { found.menimbang = true; }
            if title.eq_ignore_ascii_case("Mengingat") { found.mengingat = true; }
            out.push(format!("## {}", title));
            continue;
        }
        if let Some(cap) = re_bab.captures(line) {
            found.bab += 1;
            let roman = cap.get(1).unwrap().as_str();
            let rest = cap.get(2).map(|m| m.as_str()).unwrap_or("");
            out.push(format!("## BAB {}{}", roman, rest));
            continue;
        }
        if let Some(cap) = re_pasal.captures(line) {
            found.pasal += 1;
            let num = cap.get(1).unwrap().as_str();
            out.push(format!("## Pasal {}", num));
            continue;
        }
        if re_penj.is_match(line) {
            found.penjelasan = true;
            out.push("## PENJELASAN".to_string());
            continue;
        }
        if let Some(cap) = re_rom_sub.captures(line) {
            let roman = cap.get(1).unwrap().as_str();
            let title = cap.get(2).unwrap().as_str();
            out.push(format!("### {}. {}", roman, title));
            continue;
        }
        out.push(line.to_string());
    }

    PromoteOutput { markdown: out.join("\n"), found }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Metrics {
    pub character_coverage: f64,
    pub leak_rate: f64,
    pub split_violations: usize,
}

/// Compute coverage, leak rate, and split violations.
pub fn compute_metrics(raw_text: &str, markdown: &str, _found: &Found) -> Metrics {
    // Coverage: non-whitespace ratio
    let nw = |s: &str| s.chars().filter(|c| !c.is_whitespace()).count() as f64;
    let raw_nw = nw(raw_text);
    let md_nw = nw(markdown);
    let character_coverage = if raw_nw > 0.0 { (md_nw / raw_nw).min(1.0) } else { 0.0 };

    // Leak rate: fraction of header/footer lines remaining among total detected in raw + remaining
    let header_re = Regex::new(r"(?mi)^\s*(TAMBAHAN\s+)?LEMBARAN\s+NEGARA\s+REPUBLIK\s+INDONESIA.*$").unwrap();
    let footer_re = Regex::new(r"(?m)^\s*-\s*\d+\s*-\s*$|^\s*[\u2012\u2013\u2014\u2212\-]{1,3}\s*\d+\s*[\u2012\u2013\u2014\u2212\-]{1,3}\s*$|(?mi)^\s*(Hal(?:\.|aman))\s*\d+\s*$").unwrap();

    let count_matches = |s: &str, re: &Regex| -> usize { s.lines().filter(|l| re.is_match(l)).count() };
    let raw_headers = count_matches(raw_text, &header_re);
    let raw_footers = count_matches(raw_text, &footer_re);
    let md_headers = count_matches(markdown, &header_re);
    let md_footers = count_matches(markdown, &footer_re);

    let detected_total = raw_headers + raw_footers + md_headers + md_footers; // include remaining to avoid div-by-zero
    let leak_rate = if detected_total > 0 {
        (md_headers + md_footers) as f64 / detected_total as f64
    } else {
        0.0
    };

    // Split violations: simple heuristics
    let re_split_paren = Regex::new(r"\(\s*\n\s*\d+\)").unwrap();
    let re_line_just_letter = Regex::new(r"(?m)^\s*[a-z]\.\s*$").unwrap();
    let re_line_just_number = Regex::new(r"(?m)^\s*\d+\.\s*$").unwrap();
    let split_violations = re_split_paren.find_iter(markdown).count()
        + re_line_just_letter.find_iter(markdown).count()
        + re_line_just_number.find_iter(markdown).count();

    Metrics { character_coverage, leak_rate, split_violations }
}

#[derive(Debug, Error)]
pub enum EmitError {
    #[error("WriteFailed: {0}")]
    WriteFailed(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmitPaths {
    pub md_path: String,
    pub meta_path: String,
}

/// Atomically write markdown and meta JSON into outdir with doc_id stem.
pub fn emit_files(markdown: &str, meta: &serde_json::Value, outdir: &str, doc_id: &str) -> Result<EmitPaths, EmitError> {
    std::fs::create_dir_all(outdir).map_err(|e| EmitError::WriteFailed(e.to_string()))?;
    let md_path = Path::new(outdir).join(format!("{}.md", doc_id));
    let meta_path = Path::new(outdir).join(format!("{}.meta.json", doc_id));

    // Write temp files then rename
    let pid = std::process::id();
    let md_tmp = md_path.with_extension(format!("md.tmp.{}", pid));
    let meta_tmp = meta_path.with_extension(format!("meta.json.tmp.{}", pid));

    std::fs::write(&md_tmp, markdown).map_err(|e| EmitError::WriteFailed(e.to_string()))?;
    let meta_bytes = serde_json::to_vec_pretty(meta).map_err(|e| EmitError::WriteFailed(e.to_string()))?;
    std::fs::write(&meta_tmp, meta_bytes).map_err(|e| EmitError::WriteFailed(e.to_string()))?;

    std::fs::rename(&md_tmp, &md_path).map_err(|e| EmitError::WriteFailed(e.to_string()))?;
    std::fs::rename(&meta_tmp, &meta_path).map_err(|e| EmitError::WriteFailed(e.to_string()))?;

    Ok(EmitPaths { md_path: md_path.to_string_lossy().to_string(), meta_path: meta_path.to_string_lossy().to_string() })
}

// Utility to compute sha256 hex
pub fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let out = hasher.finalize();
    out.iter().map(|b| format!("{:02x}", b)).collect()
}
