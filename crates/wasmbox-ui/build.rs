fn main() {
    slint_build::compile_with_config(
        "sandbox.slint",
        slint_build::CompilerConfiguration::new()
            .embed_resources(slint_build::EmbedResourcesKind::EmbedForSoftwareRenderer),
    )
    .expect("compilazione .slint");
}
