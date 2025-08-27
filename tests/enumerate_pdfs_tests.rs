use std::fs;
use std::path::PathBuf;

use legalpdf_to_md::enumerate_pdfs;

#[test]
fn enumerate_pdfs_finds_nested_files() {
    let td = tempfile::tempdir().unwrap();
    let base = td.path();
    let uu_dir = base.join("input/uu");
    fs::create_dir_all(&uu_dir).unwrap();
    let f1 = uu_dir.join("A-2020.pdf");
    fs::write(&f1, b"%PDF-1.4\n").unwrap();

    let pattern = format!("{}/input/**/*.pdf", base.display());
    let files = enumerate_pdfs(&pattern).expect("should find files");
    let files: Vec<PathBuf> = files.into_iter().map(|p| p.strip_prefix(base).unwrap().to_path_buf()).collect();
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].to_string_lossy(), "input/uu/A-2020.pdf");
}

#[test]
fn enumerate_pdfs_empty_returns_error_with_guidance() {
    let td = tempfile::tempdir().unwrap();
    let base = td.path();
    let pattern = format!("{}/input/**/*.pdf", base.display());
    let err = enumerate_pdfs(&pattern).err().expect("should be error");
    let msg = format!("{}", err);
    assert_eq!(msg, "NoFilesFound");
}

