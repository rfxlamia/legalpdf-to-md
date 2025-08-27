use legalpdf_to_md::{detect_suspect_pages, poppler_extract, PopplerError};
use std::path::PathBuf;

#[test]
fn detect_suspect_pages_flags_short_pages() {
    let pages = vec![
        "\n\n   \n".to_string(), // 0 non-ws
        "abc def".to_string(), // 6 non-ws
        "x".repeat(100),       // 100 non-ws
    ];
    let suspects = detect_suspect_pages(&pages, 64);
    assert_eq!(suspects, vec![0, 1]);
}

#[test]
fn poppler_extract_file_not_found() {
    let p = PathBuf::from("./this/does/not/exist.pdf");
    let err = poppler_extract(&p, true, true).unwrap_err();
    match err {
        PopplerError::FileNotFound(_) => {}
        _ => panic!("expected FileNotFound"),
    }
}

