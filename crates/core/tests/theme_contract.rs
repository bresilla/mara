use std::fs;
use std::path::{Path, PathBuf};

use mara_core::style::{
    GraphCanvasPattern, Mode, TabLayout, ViewSwitcherLayout, theme_flat, theme_game, theme_pro,
};

#[test]
fn renderers_do_not_branch_on_theme_name() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut offenders = Vec::new();
    visit_rs_files(&root, &mut |path| {
        let rel = path.strip_prefix(&root).unwrap_or(path);
        if is_allowed_theme_identity_file(rel) {
            return;
        }
        let Ok(text) = fs::read_to_string(path) else {
            return;
        };
        for (idx, line) in text.lines().enumerate() {
            let line = line.trim();
            if line.starts_with("//") || line.starts_with("///") || line.starts_with("//!") {
                continue;
            }
            if line.contains(".name.starts_with")
                || line.contains("theme().name")
                || line.contains("theme_now.name")
                || line.contains("theme.name")
                || line.contains("\"GAME\"")
                || line.contains("\"PRO\"")
            {
                offenders.push(format!("{}:{}", rel.display(), idx + 1));
            }
        }
    });

    assert!(
        offenders.is_empty(),
        "renderer code must use typed Theme fields instead of Theme::name branches:\n{}",
        offenders.join("\n")
    );
}

#[test]
fn built_in_themes_have_typed_identity_and_tab_layouts() {
    let pro = theme_pro(Mode::Dark);
    let game = theme_game(Mode::Dark);
    let flat = theme_flat(Mode::Dark);

    assert_eq!(pro.id.family, "PRO");
    assert_eq!(game.id.family, "GAME");
    assert_eq!(flat.id.family, "FLAT");

    assert_eq!(pro.tabs.layout, TabLayout::FolderSideStrip);
    assert_eq!(game.tabs.layout, TabLayout::TitleRowSegmented);
    assert_eq!(flat.tabs.layout, TabLayout::FolderSideStrip);
    assert!(
        pro.tabs.strip_thickness - pro.tabs.folder_icon_size >= 12.0,
        "PRO side tabs need at least 6 px horizontal padding around the icon"
    );
    assert!(
        pro.tabs.tab_len - pro.tabs.folder_icon_size >= 16.0,
        "PRO side tabs need at least 8 px vertical padding around the icon"
    );

    assert!(matches!(
        pro.graph.canvas_pattern,
        GraphCanvasPattern::Dots { .. }
    ));
    assert!(matches!(
        game.graph.canvas_pattern,
        GraphCanvasPattern::Hex { .. }
    ));
    assert!(pro.widgets.progress.segments > 0);
    assert!(game.widgets.progress.segmented);
    assert!(!flat.motion.animations_enabled);
    assert_eq!(flat.widgets.button.tint_rest, 0.0);
    assert!(pro.overlay.fullscreen_button_size > 0.0);
    assert!(game.pod.widget_spacing > 0.0);
    assert!(pro.icons.overlay_icon_scale > 0.0);
    assert!(flat.icons.section_chevron_w > 0.0);
    assert_eq!(pro.views.switcher_layout, ViewSwitcherLayout::Horizontal);
    assert_eq!(game.views.switcher_layout, ViewSwitcherLayout::VerticalRail);
    assert!(pro.modules.allow_fullscreen_by_default);
    assert_eq!(pro.modules.workspace_restore_icon, "arrow-minimize");
    assert!(pro.ribbon.permanent.button_size > 0.0);
    assert!(game.ribbon.workspace.ghost_fill_alpha >= game.ribbon.view_local.ghost_fill_alpha);
    assert!(flat.ribbon.slot_override_transition >= 0.0);

    assert_ne!(pro.name, game.name);
    assert_ne!(pro.name, flat.name);
    assert_ne!(game.name, flat.name);
}

fn is_allowed_theme_identity_file(path: &Path) -> bool {
    path == Path::new("style.rs") || path.starts_with("themes")
}

fn visit_rs_files(dir: &Path, f: &mut impl FnMut(&Path)) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path: PathBuf = entry.path();
        if path.is_dir() {
            visit_rs_files(&path, f);
        } else if path.extension().and_then(|s| s.to_str()) == Some("rs") {
            f(&path);
        }
    }
}
