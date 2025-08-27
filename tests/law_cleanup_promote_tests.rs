use legalpdf_to_md::{law_cleanup, promote_legal_headings};

#[test]
fn cleanup_removes_headers_and_joins() {
    let input = "PRESIDEN REPUBLIK INDONESIA\nAlinea berakhir\npada baris\n- 2 -\nBerikutnya.";
    let out = law_cleanup(input, "auto");
    assert_eq!(out.stats.removed_header, 1);
    assert_eq!(out.stats.removed_footer, 1);
    assert!(out.cleaned.contains("Alinea berakhir pada baris"));
    assert!(out.cleaned.contains("Berikutnya."));
}

#[test]
fn promote_detects_pasal_bab_and_sections() {
    let input = "BAB I KETENTUAN UMUM\nPasal 1\nMenimbang:\nMengingat:\nPENJELASAN\nI. UMUM";
    let md = promote_legal_headings(input, "auto");
    assert!(md.markdown.contains("## BAB I KETENTUAN UMUM"));
    assert!(md.markdown.contains("## Pasal 1"));
    assert!(md.markdown.contains("## Menimbang"));
    assert!(md.markdown.contains("## Mengingat"));
    assert!(md.markdown.contains("## PENJELASAN"));
    assert!(md.markdown.contains("### I. UMUM"));
    assert!(md.found.pasal >= 1);
    assert!(md.found.bab >= 1);
    assert!(md.found.menimbang);
    assert!(md.found.mengingat);
    assert!(md.found.penjelasan);
}

