fn main() {
    // Fluent is Slint's default style on Windows; pin it so Linux `cargo check`
    // compiles exactly the widget code the owner runs on Windows.
    let config = slint_build::CompilerConfiguration::new().with_style("fluent".into());
    slint_build::compile_with_config("ui/app.slint", config).expect("Slint compile failed");
}
