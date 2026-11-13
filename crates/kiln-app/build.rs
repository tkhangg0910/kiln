//! Embeds the icon and the application manifest into kiln.exe (Windows only).

fn main() {
    println!("cargo:rerun-if-changed=res/kiln.rc");
    println!("cargo:rerun-if-changed=res/kiln.ico");
    println!("cargo:rerun-if-changed=res/kiln.manifest");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        // Only the binary gets the resources; tests and the relay check don't need them.
        embed_resource::compile_for("res/kiln.rc", ["kiln"], embed_resource::NONE).manifest_required().unwrap();
    }
}
