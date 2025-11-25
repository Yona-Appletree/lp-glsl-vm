use glsl::{parser::Parse, syntax::TranslationUnit};

#[test]
fn test_glsl_parser() {
    // Test that we can parse a simple GLSL fragment shader
    let glsl_code = r#"
        void main() {
            gl_FragColor = vec4(1.0, 0.5, 0.25, 1.0);
        }
    "#;

    let result = TranslationUnit::parse(glsl_code);
    assert!(result.is_ok(), "GLSL parsing failed: {:?}", result.err());

    let translation_unit = result.unwrap();
    // TranslationUnit contains a NonEmpty<ExternalDeclaration>, which always has at least one element
    // NonEmpty wraps a Vec, so we access it via .0
    assert!(
        !translation_unit.0 .0.is_empty(),
        "Translation unit should not be empty"
    );
}
