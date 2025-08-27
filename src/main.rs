use std::path::Path;

use legalpdf_to_md::{check_deps, compute_metrics, detect_suspect_pages, emit_files, enumerate_pdfs, law_cleanup, merge_pages, nala_help_for, ocr_tesseract, poppler_extract, promote_legal_headings, suppress_repeated_lines, validate_prd, DepsResult, PopplerError, SuppressorConfig, sha256_hex};
use std::fs;
use std::collections::HashSet;
use regex::Regex;

fn main() {
    // Simple CLI flags parsing
    let args: Vec<String> = std::env::args().collect();
    let dump_steps = args.iter().any(|a| a == "--dump-steps");
    // OCR flag supports: --with-ocr, --with-ocr=on, --with-ocr=off
    let mut with_ocr_forced: Option<bool> = None;
    if let Some(pos) = args.iter().position(|a| a.starts_with("--with-ocr")) {
        let val = &args[pos];
        if val == "--with-ocr" || val == "--with-ocr=on" { with_ocr_forced = Some(true); }
        else if val == "--with-ocr=off" { with_ocr_forced = Some(false); }
    }
    let strict = args.iter().any(|a| a == "--strict");
    let mut law_mode = String::from("auto");
    if let Some(pos) = args.iter().position(|a| a == "--law-mode") {
        if let Some(val) = args.get(pos + 1) {
            if !val.starts_with("--") {
                law_mode = val.clone();
            }
        }
    }
    let mut ocr_lang = String::from("ind");
    if let Some(pos) = args.iter().position(|a| a == "--ocr-lang") {
        if let Some(val) = args.get(pos + 1) {
            if !val.starts_with("--") {
                ocr_lang = val.clone();
            }
        }
    }
    // OCR DPI
    let mut ocr_dpi: u32 = 300;
    if let Some(pos) = args.iter().position(|a| a == "--ocr-dpi") {
        if let Some(val) = args.get(pos + 1) {
            if let Ok(n) = val.parse::<u32>() { ocr_dpi = n.max(72); }
        }
    }
    // Minor patch flags and helpers
    let mut artifacts_on = false; // default off
    if let Some(val) = args.iter().find(|a| a.starts_with("--artifacts")) {
        if let Some(eqpos) = val.find('=') {
            let v = &val[eqpos + 1..];
            artifacts_on = v == "on";
        }
    }
    let mut per_doc_dir_on = true; // default on
    if let Some(val) = args.iter().find(|a| a.starts_with("--per-doc-dir")) {
        if let Some(eqpos) = val.find('=') {
            let v = &val[eqpos + 1..];
            per_doc_dir_on = v != "off";
        }
    }

    // Track used slugs for uniqueness
    let mut used_doc_ids: HashSet<String> = HashSet::new();

    fn slugify(base: &str) -> String {
        let lower = base.to_lowercase();
        let mut s = String::with_capacity(lower.len());
        for ch in lower.chars() {
            if ch.is_ascii_alphanumeric() {
                s.push(ch);
            } else {
                s.push('-');
            }
        }
        let trimmed = s.trim_matches('-').to_string();
        let mut collapsed = String::with_capacity(trimmed.len());
        let mut prev_dash = false;
        for ch in trimmed.chars() {
            if ch == '-' {
                if !prev_dash {
                    collapsed.push(ch);
                }
                prev_dash = true;
            } else {
                prev_dash = false;
                collapsed.push(ch);
            }
        }
        if collapsed.is_empty() {
            "doc".to_string()
        } else {
            collapsed
        }
    }

    fn unique_slug(slug_in: String, used: &mut HashSet<String>) -> String {
        if !used.contains(&slug_in) {
            used.insert(slug_in.clone());
            return slug_in;
        }
        let mut i = 1;
        loop {
            let candidate = format!("{}-{}", slug_in, i);
            if !used.contains(&candidate) {
                used.insert(candidate.clone());
                return candidate;
            }
            i += 1;
        }
    }
    // 1) Read and validate prd.yaml
    let prd_path = Path::new("prd.yaml");
    let prd = match validate_prd(prd_path) {
        Ok(p) => p,
        Err(e) => {
            eprintln!(
                "{}",
                serde_json::json!({
                    "tool": "validate_prd",
                    "file": "prd.yaml",
                    "error": e.to_string()
                })
            );
            std::process::exit(3);
        }
    };

    eprintln!(
        "{}",
        serde_json::json!({
            "tool":"validate_prd",
            "file":"prd.yaml",
            "status":"ok",
            "input_glob": prd.input_glob(),
            "output_dir": prd.output_dir()
        })
    );

    // 2) T0: check_deps
    let deps: DepsResult = check_deps();
    if !deps.ok {
        eprintln!(
            "{}",
            serde_json::json!({
                "tool":"check_deps",
                "missing": deps.missing,
                "error_code": 2
            })
        );
        let help = nala_help_for(&deps.missing);
        if !help.is_empty() {
            eprintln!("{}", help);
        }
        std::process::exit(2);
    } else {
        eprintln!(
            "{}",
            serde_json::json!({
                "tool":"check_deps",
                "status":"ok",
                "missing": deps.missing
            })
        );
        if !deps.missing.is_empty() {
            let help = nala_help_for(&deps.missing);
            if !help.is_empty() {
                eprintln!("{}", help);
            }
        }
    }

    // 3) T1: enumerate_pdfs on configured glob
    let input_glob = prd.input_glob();

    match enumerate_pdfs(&input_glob) {
        Ok(files) => {
            eprintln!(
                "{}",
                serde_json::json!({
                    "tool":"enumerate_pdfs",
                    "count": files.len(),
                })
            );

            // Process each file: T2 poppler_extract -> T3 detect_suspect_pages -> T4 (optional) OCR -> T5 merge
            for file in files {
                let started_ms = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as i128).unwrap_or(0);
                let fname = file.file_name().and_then(|s| s.to_str()).unwrap_or("doc.pdf").to_string();
                let base = fname.trim_end_matches(".pdf");
                let slug = unique_slug(slugify(base), &mut used_doc_ids);
                let doc_id = slug; // used for directories and filenames
                let base_output = prd.output_dir();
                let doc_outdir = if per_doc_dir_on { format!("{}/{}", base_output, doc_id) } else { base_output.clone() };
                let artifacts_dir = if artifacts_on || dump_steps { Some(format!("{}/artifacts", doc_outdir)) } else { None };
                match poppler_extract(&file, true, true) {
                    Ok(pages) => {
                        eprintln!(
                            "{}",
                            serde_json::json!({
                                "tool":"poppler_extract",
                                "file": file,
                                "pages": pages.len()
                            })
                        );
                        if let Some(ad) = &artifacts_dir {
                            let joined = pages.join("\n");
                            let _ = std::fs::create_dir_all(ad);
                            let step_path = format!("{}/step1_extract.txt", ad);
                            if let Err(e) = fs::write(&step_path, joined) {
                                eprintln!(
                                    "{}",
                                    serde_json::json!({
                                        "tool":"dump_steps",
                                        "file": step_path,
                                        "error": e.to_string()
                                    })
                                );
                            }
                        }
                        let page_count = pages.len();
                        let mut suspects = detect_suspect_pages(&pages, 64);
                        // CI sampling: restrict suspect pages to first N via env CI_SAMPLE_SUSPECTS
                        if let Ok(sample_n) = std::env::var("CI_SAMPLE_SUSPECTS").and_then(|v| v.parse::<usize>().map_err(|_| std::env::VarError::NotPresent)) {
                            if sample_n > 0 && suspects.len() > sample_n { suspects.truncate(sample_n); }
                        }
                        eprintln!(
                            "{}",
                            serde_json::json!({
                                "tool":"detect_suspect_pages",
                                "file": file,
                                "suspect_pages": suspects
                            })
                        );

                        // Enforce OCR for suspect pages when deps available (Minor-Patch-III)
                        let has_tesseract = which::which("tesseract").is_ok() && which::which("pdftoppm").is_ok();
                        let ocr_enabled = has_tesseract; // enabled if deps available
                        let ocr_requested = with_ocr_forced.unwrap_or(!suspects.is_empty()); // auto when suspects exist

                        let mut ocr_ran = false;
                        let mut ocr_run_pages: Vec<usize> = Vec::new();
                        let mut ocr_skipped_reason: Option<String> = None;
                        let ocr_lang_used = ocr_lang.clone();
                        let ocr_psm: u8 = 4;
                        let ocr_oem: u8 = 1;
                        let ocr_dpi: u32 = ocr_dpi;
                        let mut pages_after_ocr = pages.clone();
                        if ocr_enabled && ocr_requested && !suspects.is_empty() {
                            let ad_path = artifacts_dir.as_ref().map(|s| std::path::Path::new(s).to_path_buf());
                            let ocr = if let Some(p) = &ad_path { ocr_tesseract(&file, &suspects, &ocr_lang_used, ocr_dpi, Some(p.as_path()), ocr_psm, ocr_oem) } else { ocr_tesseract(&file, &suspects, &ocr_lang_used, ocr_dpi, None, ocr_psm, ocr_oem) };
                            eprintln!(
                                "{}",
                                serde_json::json!({
                                    "tool":"ocr_tesseract",
                                    "file": file,
                                    "attempted": suspects.len(),
                                    "texts": ocr.texts.len(),
                                    "failed": ocr.failed,
                                    "skipped_due_to_missing_deps": ocr.skipped_due_to_missing_deps,
                                    "lang": ocr_lang_used
                                })
                            );
                            if !ocr.skipped_due_to_missing_deps {
                                for t in &ocr.texts {
                                    if let Some(slot) = pages_after_ocr.get_mut(t.index) {
                                        *slot = t.text.clone();
                                    }
                                }
                                ocr_ran = true;
                                ocr_run_pages = ocr.texts.iter().map(|t| t.index).collect();
                                // Write OCR summary when artifacts on
                                if let Some(ad) = &artifacts_dir {
                                    let ocr_dir = format!("{}/ocr", ad);
                                    let _ = std::fs::create_dir_all(&ocr_dir);
                                    let mut summary = String::new();
                                    summary.push_str(&format!("attempted: {}\n", suspects.len()));
                                    summary.push_str(&format!("success: {}\n", ocr.texts.len()));
                                    summary.push_str(&format!("failed: {}\n", ocr.failed.len()));
                                    if !ocr.failed.is_empty() { summary.push_str(&format!("failed_indices: {:?}\n", ocr.failed)); }
                                    if !ocr.errors.is_empty() {
                                        summary.push_str("errors:\n");
                                        for e in &ocr.errors { summary.push_str(&format!("- page_index={} error={}\n", e.index, e.message)); }
                                    }
                                    let _ = std::fs::write(format!("{}/ocr_summary.txt", ocr_dir), summary);
                                }
                            } else {
                                ocr_skipped_reason = Some("tesseract_missing".to_string());
                            }
                        } else if !ocr_enabled && !suspects.is_empty() {
                            ocr_skipped_reason = Some("tesseract_missing".to_string());
                        } else if with_ocr_forced == Some(false) && !suspects.is_empty() {
                            ocr_skipped_reason = Some("disabled_by_flag".to_string());
                        }

                        // Persist step2_merge.txt (OCR overrides merged) if artifacts on
                        if let Some(ad) = &artifacts_dir {
                            let _ = std::fs::create_dir_all(ad);
                            let step2_path = format!("{}/step2_merge.txt", ad);
                            let merged_preview = pages_after_ocr.join("\n");
                            let _ = fs::write(&step2_path, merged_preview);
                        }

                        // Apply repeated-line suppressor on a per-page basis before cleanup
                        let keep_lines_regex = args.iter().position(|a| a == "--keep-lines").and_then(|i| args.get(i+1)).and_then(|p| Regex::new(p).ok());
                        let cfg = SuppressorConfig { threshold_ratio: 0.60, keep_lines: keep_lines_regex };
                        let (suppressed_pages, suppress_stats, removed_candidates) = suppress_repeated_lines(&pages_after_ocr, &cfg);
                        if let Some(ad) = &artifacts_dir {
                            // Dump preview
                            let _ = std::fs::create_dir_all(ad);
                            let prev = format!("{}/suppressor_preview.txt", ad);
                            let _ = fs::write(&prev, removed_candidates.join("\n"));
                        }
                        // Merge suppressed pages (already contained OCR overrides) for cleanup/metrics
                        let merged = merge_pages(&suppressed_pages, &[]);
                        if let Some(ad) = &artifacts_dir {
                            let _ = std::fs::create_dir_all(ad);
                            let step2_path = format!("{}/step2_merge.txt", ad);
                            if let Err(e) = fs::write(&step2_path, &merged) {
                                eprintln!(
                                    "{}",
                                    serde_json::json!({
                                        "tool":"dump_steps",
                                        "file": step2_path,
                                        "error": e.to_string()
                                    })
                                );
                            }
                        }
                        eprintln!(
                            "{}",
                            serde_json::json!({
                                "tool":"merge_pages",
                                "file": file,
                                "length": merged.len()
                            })
                        );

                        // T6: Cleanup
                        let mut cleaned = law_cleanup(&merged, &law_mode);
                        // Merge suppressor stats into cleanup stats for meta
                        cleaned.stats.removed_header += suppress_stats.removed_header;
                        cleaned.stats.removed_footer += suppress_stats.removed_footer;
                        cleaned.stats.removed_lines_sample = suppress_stats.removed_lines_sample;
                        cleaned.stats.suppressor_overrun = suppress_stats.suppressor_overrun;
                        eprintln!(
                            "{}",
                            serde_json::json!({
                                "tool":"law_cleanup",
                                "file": file,
                                "removed_header": cleaned.stats.removed_header,
                                "removed_footer": cleaned.stats.removed_footer,
                                "hyphens_fixed": cleaned.stats.hyphens_fixed
                            })
                        );

                        // T7: Promote headings
                        let promoted = promote_legal_headings(&cleaned.cleaned, &law_mode);
                        if let Some(ad) = &artifacts_dir {
                            let _ = std::fs::create_dir_all(ad);
                            let step3_path = format!("{}/step3_md.txt", ad);
                            if let Err(e) = fs::write(&step3_path, &promoted.markdown) {
                                eprintln!(
                                    "{}",
                                    serde_json::json!({
                                        "tool":"dump_steps",
                                        "file": step3_path,
                                        "error": e.to_string()
                                    })
                                );
                            }
                        }
                        eprintln!(
                            "{}",
                            serde_json::json!({
                                "tool":"promote_legal_headings",
                                "file": file,
                                "found": promoted.found
                            })
                        );

                        // Strict mode enforcement for PP/Permen
                        if strict {
                            let lm = law_mode.to_lowercase();
                            if (lm == "pp" || lm == "permen") && (promoted.found.pasal == 0 || promoted.found.bab == 0) {
                                eprintln!(
                                    "{}",
                                    serde_json::json!({
                                        "tool":"promote_legal_headings",
                                        "file": file,
                                        "error":"StructureNotFound",
                                        "error_code": 5,
                                        "found": promoted.found
                                    })
                                );
                                std::process::exit(5);
                            }
                        }

                        // T8: Metrics
                        let metrics = compute_metrics(&merged, &promoted.markdown, &promoted.found);
                        eprintln!(
                            "{}",
                            serde_json::json!({
                                "tool":"compute_metrics",
                                "file": file,
                                "character_coverage": metrics.character_coverage,
                                "leak_rate": metrics.leak_rate,
                                "split_violations": metrics.split_violations
                            })
                        );

                        // T9: Emit files (atomic)
                        let finished_ms = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as i128).unwrap_or(0);
                        // Timing vector proxy & p95
                        let total_ms = (finished_ms - started_ms).max(0) as u128;
                        let per_page = if page_count>0 { (total_ms / (page_count as u128)) as u64 } else { 0 };
                        let timing_ms_per_page: Vec<u64> = vec![per_page; page_count];
                        let p95_latency_ms_per_page: u64 = per_page;
                        // coverage_pages metric
                        let suspects_len = suspects.len() as i64;
                        let run_len = ocr_run_pages.len() as i64;
                        let pages_i = page_count as i64;
                        let cov_pages = if pages_i > 0 { 1.0 - (((suspects_len - run_len).max(0) as f64) / (pages_i as f64)) } else { 0.0 };

                        let meta = serde_json::json!({
                            "doc_id": doc_id,
                            "engine": "poppler",
                            "suspect_pages": suspects,
                            "ocr": {
                                "enabled": ocr_enabled,
                                "ran": ocr_ran,
                                "skipped_reason": ocr_skipped_reason,
                                "ocr_run_pages": ocr_run_pages,
                                "lang": ocr_lang_used,
                                "psm": ocr_psm,
                                "oem": ocr_oem,
                                "dpi": ocr_dpi,
                            },
                            "found": promoted.found,
                            "stats": cleaned.stats,
                            "metrics": {
                                "character_coverage": metrics.character_coverage,
                                "leak_rate": metrics.leak_rate,
                                "split_violations": metrics.split_violations,
                                "coverage_pages": cov_pages
                            },
                            "page_count": page_count,
                            "timing_ms_per_page": timing_ms_per_page,
                            "p95_latency_ms_per_page": p95_latency_ms_per_page,
                            "timestamps": {"started_ms": started_ms, "finished_ms": finished_ms},
                        });
                        // Compute meta_fingerprint (normalized meta without timestamps)
                        let mut meta_norm = meta.clone();
                        if let Some(obj) = meta_norm.as_object_mut() {
                            obj.remove("timestamps");
                        }
                        let meta_norm_bytes = serde_json::to_vec(&meta_norm).unwrap_or_default();
                        let fingerprint = sha256_hex(&meta_norm_bytes);
                        let mut meta_full = meta.as_object().cloned().unwrap_or_default();
                        meta_full.insert("meta_fingerprint".to_string(), serde_json::json!(fingerprint));
                        let meta = serde_json::Value::Object(meta_full);
                        // Ensure doc output directory exists
                        let _ = std::fs::create_dir_all(&doc_outdir);
                        match emit_files(&promoted.markdown, &meta, doc_outdir.as_str(), &doc_id) {
                            Ok(paths) => {
                                eprintln!(
                                    "{}",
                                    serde_json::json!({
                                        "tool":"emit_files",
                                        "file": file,
                                        "md_path": paths.md_path,
                                        "meta_path": paths.meta_path
                                    })
                                );
                            }
                            Err(e) => {
                                eprintln!(
                                    "{}",
                                    serde_json::json!({
                                        "tool":"emit_files",
                                        "file": file,
                                        "error": e.to_string(),
                                        "error_code": 6
                                    })
                                );
                                std::process::exit(6);
                            }
                        }
                    }
                    Err(err) => {
                        let (code, label) = match err {
                            PopplerError::FileNotFound(_) => (1, "FileNotFound"),
                            PopplerError::EncryptedPDF(_) => (1, "EncryptedPDF"),
                            PopplerError::Other(_) => (1, "PopplerError"),
                        };
                        eprintln!(
                            "{}",
                            serde_json::json!({
                                "tool":"poppler_extract",
                                "file": file,
                                "error": label,
                                "error_code": code
                            })
                        );
                        std::process::exit(code);
                    }
                }
            }
        }
        Err(err) => {
            let guidance = match err {
                legalpdf_to_md::EnumerateError::NoFilesFound { guidance } => guidance,
            };
            eprintln!(
                "{}",
                serde_json::json!({
                    "tool":"enumerate_pdfs",
                    "error":"NoFilesFound",
                    "error_code":1
                })
            );
            // Spec: still print folder guidance
            eprintln!("{}", guidance);
            std::process::exit(1);
        }
    }
}
