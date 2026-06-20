use std::env;

fn main() {
    println!("cargo:rerun-if-changed=app.rc");
    println!("cargo:rerun-if-changed=app.manifest");
    println!("cargo:rerun-if-changed=icon.ico");
    println!("cargo:rerun-if-env-changed=J3LAUNCHER_SKIP_WINDOWS_RESOURCES");

    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if target_os != "windows" {
        return;
    }
    if env::var_os("J3LAUNCHER_SKIP_WINDOWS_RESOURCES").is_some() {
        return;
    }

    if let Err(error) = embed_resource::compile("app.rc", embed_resource::NONE).manifest_required()
    {
        panic!("failed to compile Windows resources: {error}");
    }
}
