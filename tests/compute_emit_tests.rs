use legalpdf_to_md::{compute_metrics, emit_files, law_cleanup, merge_pages, promote_legal_headings, Metrics};
use std::fs;
use std::hash::{Hash, Hasher};
use std::collections::hash_map::DefaultHasher;

fn hash_u64<T: Hash>(t: &T) -> u64 {
    let mut s = DefaultHasher::new();
    t.hash(&mut s);
    s.finish()
}

#[test]
fn metrics_basic_and_emit_files() {
    let pages = vec![
        "PRESIDEN REPUBLIK INDONESIA\nBAB I KETENTUAN UMUM\nPasal 1\nHuruf a. contoh\n- 1 -".to_string(),
    ];
    let merged = merge_pages(&pages, &[]);
    let cleaned = law_cleanup(&merged, "auto");
    let promoted = promote_legal_headings(&cleaned.cleaned, "auto");
    let metrics = compute_metrics(&merged, &promoted.markdown, &promoted.found);

    assert!(metrics.character_coverage > 0.0 && metrics.character_coverage <= 1.0);

    // Emit files
    let td = tempfile::tempdir().unwrap();
    let outdir = td.path().join("out");
    let meta = serde_json::json!({
        "doc_id": "doc.pdf",
        "engine": "poppler",
        "suspect_pages": [],
        "ocr": {"enabled": false, "ran": false, "skipped_reason": "disabled_by_flag", "ocr_run_pages": []},
        "found": promoted.found,
        "stats": cleaned.stats,
        "metrics": metrics,
        "timestamps": {"started_ms": 1, "finished_ms": 2},
    });
    let paths = emit_files(&promoted.markdown, &meta, outdir.to_str().unwrap(), "doc.pdf").expect("emit ok");
    let md = fs::read_to_string(paths.md_path).unwrap();
    let m = fs::read_to_string(paths.meta_path).unwrap();
    assert!(m.contains("\"doc_id\""));
    assert_eq!(md, promoted.markdown);
}

#[test]
fn idempotent_md_hash_same_runs() {
    let pages = vec![
        "BAB I KETENTUAN UMUM\nPasal 1\nI. UMUM\na. Hal\n1. Angka".to_string(),
    ];
    // First run
    let merged1 = merge_pages(&pages, &[]);
    let cleaned1 = law_cleanup(&merged1, "auto");
    let promoted1 = promote_legal_headings(&cleaned1.cleaned, "auto");
    // Second run
    let merged2 = merge_pages(&pages, &[]);
    let cleaned2 = law_cleanup(&merged2, "auto");
    let promoted2 = promote_legal_headings(&cleaned2.cleaned, "auto");

    let h1 = hash_u64(&promoted1.markdown);
    let h2 = hash_u64(&promoted2.markdown);
    assert_eq!(h1, h2, "Markdown must be idempotent across runs");
}
