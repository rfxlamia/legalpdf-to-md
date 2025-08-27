use std::fs;
use std::os::unix::fs::PermissionsExt;

use legalpdf_to_md::check_deps;

fn set_path(dir: &std::path::Path) {
    std::env::set_var("PATH", dir.display().to_string());
}

#[test]
fn check_deps_ok_when_pdftotext_present() {
    let td = tempfile::tempdir().unwrap();
    let fake_bin = td.path().join("pdftotext");
    fs::write(&fake_bin, "#!/bin/sh\nexit 0\n").unwrap();
    let mut perms = fs::metadata(&fake_bin).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&fake_bin, perms).unwrap();

    set_path(td.path());
    let res = check_deps();
    assert!(res.ok, "pdftotext present should yield ok");
    // tesseract likely missing in test PATH
    assert!(res.missing.iter().any(|m| m == "tesseract"));
}

#[test]
fn check_deps_missing_required_dep() {
    let td = tempfile::tempdir().unwrap();
    set_path(td.path()); // empty PATH
    let res = check_deps();
    assert!(!res.ok, "missing pdftotext should not be ok");
    assert!(res.missing.iter().any(|m| m == "pdftotext"));
}

