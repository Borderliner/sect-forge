fn main() {
    println!("cargo:rerun-if-changed=src/data/sect-forge.rc");
    println!("cargo:rerun-if-changed=src/data/sect-forge.ico");

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows")
        && std::env::var_os("CARGO_FEATURE_DESKTOP").is_some()
    {
        embed_resource::compile_for(
            "src/data/sect-forge.rc",
            ["sect-forge"],
            embed_resource::NONE,
        )
        .manifest_required()
        .expect("embed the Sect Forge Windows icon");
    }
}
