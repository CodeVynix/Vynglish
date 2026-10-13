use vynglish::generate_docs;

#[test]
fn docs_md_is_up_to_date() {
    let generated = generate_docs();
    let checked_in = std::fs::read_to_string("docs.md").unwrap_or_default();

    // Normalize line endings
    let generated = generated.replace("\r\n", "\n");
    let checked_in = checked_in.replace("\r\n", "\n");

    assert_eq!(
        generated, checked_in,
        "docs.md is out of date. Run `vyn --gen-docs` to update."
    );
}
