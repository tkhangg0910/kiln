//! Embeds the icon and the application manifest into kilno.exe (Windows only).

fn main() {
    println!("cargo:rerun-if-changed=res/kilno.rc");
    println!("cargo:rerun-if-changed=res/kilno.ico");
    println!("cargo:rerun-if-changed=res/kilno.manifest");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        // Only the binary gets the resources; tests and the relay check don't need them.
        embed_resource::compile_for("res/kilno.rc", ["kilno"], embed_resource::NONE).manifest_required().unwrap();
    }
}
