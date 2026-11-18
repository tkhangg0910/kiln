//! Embeds the icon, the application manifest and the version info into kilno.exe (Windows only).
//! The .rc is written here so its version is Cargo's: an exe with no name, maker or version
//! scores worse with Windows Defender's machine-learning checks (`…!ml`).

use std::path::Path;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=res/kilno.ico");
    println!("cargo:rerun-if-changed=res/kilno.manifest");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let env = |k: &str| std::env::var(k).expect(k);
    let res = Path::new(&env("CARGO_MANIFEST_DIR")).join("res");
    // An .rc string doubles its backslashes.
    let at = |f: &str| res.join(f).display().to_string().replace('\\', "\\\\");
    let (v, major, minor, patch) =
        (env("CARGO_PKG_VERSION"), env("CARGO_PKG_VERSION_MAJOR"), env("CARGO_PKG_VERSION_MINOR"), env("CARGO_PKG_VERSION_PATCH"));
    // Icon 1 (Explorer, the taskbar, win::shell::set_window_icon), the manifest (PerMonitorV2,
    // long paths, Windows 10+) and what Explorer's Details tab shows.
    let rc = format!(
        r#"1 ICON "{ico}"
1 24 "{manifest}"
1 VERSIONINFO
FILEVERSION {major},{minor},{patch},0
PRODUCTVERSION {major},{minor},{patch},0
FILEOS 0x40004
FILETYPE 0x1
BEGIN
  BLOCK "StringFileInfo"
  BEGIN
    BLOCK "040904b0"
    BEGIN
      VALUE "CompanyName", "Kilno (github.com/tkhangg0910/kilno)"
      VALUE "FileDescription", "Kilno"
      VALUE "FileVersion", "{v}"
      VALUE "InternalName", "kilno"
      VALUE "LegalCopyright", "MIT License"
      VALUE "OriginalFilename", "kilno.exe"
      VALUE "ProductName", "Kilno"
      VALUE "ProductVersion", "{v}"
    END
  END
  BLOCK "VarFileInfo"
  BEGIN
    VALUE "Translation", 0x409, 1200
  END
END
"#,
        ico = at("kilno.ico"),
        manifest = at("kilno.manifest"),
    );
    let out = Path::new(&env("OUT_DIR")).join("kilno.rc");
    std::fs::write(&out, rc).expect("write kilno.rc");
    // Only the binary gets the resources; tests and the relay check don't need them.
    embed_resource::compile_for(&out, ["kilno"], embed_resource::NONE).manifest_required().unwrap();
}
