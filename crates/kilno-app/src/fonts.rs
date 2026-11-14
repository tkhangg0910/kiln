//! Fonts from `C:\Windows\Fonts`, memory-mapped, with a per-glyph fallback chain
//! (PORTING.md §1): terminal Cascadia Mono → Consolas, UI Segoe UI; then Segoe UI Symbol,
//! Segoe UI Emoji (monochrome, D7), Microsoft YaHei UI, Yu Gothic UI, Malgun Gothic.

use crate::win::{fonts_dir, map_font};
use egui::{FontData, FontDefinitions, FontFamily};
use std::sync::Arc;

/// Family name for semibold UI text.
pub const SEMIBOLD: &str = "semibold";

/// Family name for Segoe Fluent Icons / MDL2 glyphs.
pub const ICONS: &str = "icons";

fn add(defs: &mut FontDefinitions, name: &str, file: &str, index: u32) -> bool {
    let Some(bytes) = map_font(&fonts_dir().join(file)) else { return false };
    let mut data = FontData::from_static(bytes);
    data.index = index;
    defs.font_data.insert(name.to_string(), Arc::new(data));
    true
}

/// Font definitions with system fonts first and egui's built-ins as the last resort.
pub fn definitions() -> FontDefinitions {
    let mut defs = FontDefinitions::default();
    let mono = ["CascadiaMono.ttf", "CascadiaCode.ttf", "consola.ttf"]
        .iter()
        .find_map(|f| add(&mut defs, "mono", f, 0).then_some(()))
        .is_some();
    let ui = add(&mut defs, "ui", "segoeui.ttf", 0);
    let semi = add(&mut defs, "ui-semibold", "seguisb.ttf", 0) || add(&mut defs, "ui-semibold", "segoeuib.ttf", 0);
    let mut fallback = Vec::new();
    for (name, file) in [
        ("symbol", "seguisym.ttf"),
        ("emoji", "seguiemj.ttf"),
        ("yahei", "msyh.ttc"),
        ("yugothic", "YuGothM.ttc"),
        ("malgun", "malgun.ttf"),
    ] {
        if add(&mut defs, name, file, 0) {
            fallback.push(name.to_string());
        }
    }

    let chain = |first: &[(&str, bool)], defs: &FontDefinitions, builtin: &[String]| -> Vec<String> {
        let mut v: Vec<String> = first.iter().filter(|(_, ok)| *ok).map(|(n, _)| n.to_string()).collect();
        v.extend(fallback.iter().cloned());
        v.extend(builtin.iter().filter(|b| defs.font_data.contains_key(*b)).cloned());
        v
    };
    let builtin_mono = defs.families.get(&FontFamily::Monospace).cloned().unwrap_or_default();
    let builtin_prop = defs.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    let m = chain(&[("mono", mono)], &defs, &builtin_mono);
    let p = chain(&[("ui", ui)], &defs, &builtin_prop);
    let s = chain(&[("ui-semibold", semi), ("ui", ui)], &defs, &builtin_prop);
    // Glyph icons (E710 add, E713 gear, ...): Segoe Fluent Icons (Windows 11), else MDL2.
    let fluent = add(&mut defs, "icons-fluent", "SegoeIcons.ttf", 0);
    let mdl2 = add(&mut defs, "icons-mdl2", "segmdl2.ttf", 0);
    let i = chain(&[("icons-fluent", fluent), ("icons-mdl2", mdl2), ("ui", ui)], &defs, &builtin_prop);
    defs.families.insert(FontFamily::Monospace, m);
    defs.families.insert(FontFamily::Proportional, p);
    defs.families.insert(FontFamily::Name(SEMIBOLD.into()), s);
    defs.families.insert(FontFamily::Name(ICONS.into()), i);
    defs
}
