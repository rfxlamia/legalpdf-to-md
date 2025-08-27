use legalpdf_to_md::{merge_pages, OcrText};

#[test]
fn merge_overrides_replace_only_target_indices() {
    let pages = vec![
        "page1".to_string(),
        "page2".to_string(),
        "page3".to_string(),
    ];
    let overrides = vec![
        OcrText { index: 1, text: "OCR_PAGE2".to_string() },
    ];
    let merged = merge_pages(&pages, &overrides);
    assert_eq!(merged, "page1\nOCR_PAGE2\npage3");
}

