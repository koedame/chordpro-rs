/// Golden test for the image directive in the HTML renderer.
///
/// Verifies that `{image}` directives with various attributes (src, width,
/// height, scale, title, anchor) produce the expected HTML output.
#[test]
fn image_directive_html_golden() {
    let input = std::fs::read_to_string("tests/fixtures/image-directive/input.cho")
        .expect("read input.cho")
        .replace("\r\n", "\n");
    let expected = std::fs::read_to_string("tests/fixtures/image-directive/expected.html")
        .expect("read expected.html")
        .replace("\r\n", "\n");

    let actual = chordpro_render_html::render(&input);

    assert_eq!(
        actual, expected,
        "HTML output for image directive does not match golden snapshot.\n\
         If the change is intentional, update tests/fixtures/image-directive/expected.html"
    );
}
