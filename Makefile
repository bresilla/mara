SHELL := /bin/bash

PROJECT_NAME_FROM_CARGO := $(shell sed -n 's/^[[:space:]]*name[[:space:]]*=[[:space:]]*"\([^"]*\)".*/\1/p' Cargo.toml | head -1)
PROJECT_VERSION_FROM_CARGO := $(shell sed -n 's/^[[:space:]]*version[[:space:]]*=[[:space:]]*"\([^"]*\)".*/\1/p' Cargo.toml | head -1)
PROJECT_NAME ?= $(or $(PROJECT_NAME_FROM_CARGO),$(notdir $(CURDIR)))
PROJECT_VERSION ?= $(or $(PROJECT_VERSION_FROM_CARGO),dev)

TOP_DIR := $(CURDIR)
CARGO := cargo
# Native windowing backend used by the root winit-owned example.
BACKEND ?= wayland
DISPLAY ?= :1
APP_BIN ?= native
APP_PKG ?= mara_example
APP_TARGET := -p $(APP_PKG) --bin $(APP_BIN)
TARGET ?= native
RUN_WITH ?= nixVulkan
TYPE ?= patch
HAS_REL := $(shell command -v git-rel 2>/dev/null)

$(info ------------------------------------------)
$(info Project: $(PROJECT_NAME) v$(PROJECT_VERSION))
$(info Display: $(BACKEND) backend)
$(info ------------------------------------------)

.PHONY: build b compile c run r serve test t test-all check harden bench clean docs release help h

build:
	@if [ "$(TARGET)" = "native" ]; then \
		$(CARGO) build $(APP_TARGET); \
	elif [ "$(TARGET)" = "web" ]; then \
		cd $(WEB_DIR) && env -u NO_COLOR trunk build --release; \
	elif [ "$(TARGET)" = "apk" ]; then \
		echo "TARGET=apk is reserved for Android builds; not implemented yet."; \
		exit 1; \
	else \
		echo "Unknown TARGET=$(TARGET). Use TARGET=native, TARGET=web, or TARGET=apk."; \
		exit 2; \
	fi

b: build

compile:
	@$(CARGO) clean
	@$(MAKE) build

c: compile

run:
	@WINIT_UNIX_BACKEND=$(BACKEND) $(RUN_WITH) $(CARGO) run $(APP_TARGET)

WEB_DIR := example

serve:
	@$(MAKE) build TARGET=web
	@cd $(WEB_DIR) && trunk serve --open

r: run

test:
	@$(CARGO) test $(APP_TARGET)

t: test

test-all:
	@$(CARGO) test --workspace --all-targets

check:
	@$(CARGO) check --workspace --all-targets
	@$(CARGO) check --manifest-path example/sealed/Cargo.toml
	@! grep -n 'raw-egui' example/Cargo.toml
	@! grep -n 'raw-egui' crates/core/Cargo.toml mara/Cargo.toml
	@! grep -RInE 'cfg[(]feature[[:space:]]*=[[:space:]]*"raw-egui"|^[[:space:]]*pub[[:space:]]+use[[:space:]]+egui([:;]|$$)|^[[:space:]]*pub[[:space:]]+fn[[:space:]]+(from_raw|raw_ui_mut|raw|egui|egui_ctx|ctx)[(]' crates/core/src mara/src
	@! grep -nE '^pub type .*=[[:space:]]*egui::' crates/core/src/vocab.rs
	@! grep -nE '^(mara_core|mara_(map|canvas|image|code|graph|3d|bevy)|egui)[[:space:]]*=' example/Cargo.toml
	@! grep -RInE 'MaraUi::from_raw|host\.egui\(\)' example/src
	@! grep -RInE 'ViewCtx::new[(]ctx|host[.]__internal_egui[(][)][.]clone[(][)]|bevy_view[.]show[(]host[.]__internal_egui|canvas_root_view[(]host[.]__internal_egui|fn canvas_root_view[(][[:space:]]*ctx:[[:space:]]*&egui::Context' example/src/app.rs
	@! grep -nE '^[[:space:]]*(pub\\(crate\\)[[:space:]]+)?[A-Za-z_][A-Za-z0-9_]*:[[:space:]]*egui::Response,' crates/core/src/mui/mod.rs
	@! grep -RIn 'MaraInput::snapshot' crates/core/src
	@! grep -RIn 'ctx().data' crates/core/src/widget/color.rs
	@! grep -RInE 'ctx[.]?[(]?[)]?[.]data' crates/core/src/widget/button.rs crates/core/src/widget/color.rs
	@! grep -RInE 'CollapsingState|show_body_indented|with_response' crates/core/src/widget/foldable.rs
	@! grep -RIn 'ui\.indent' crates/core/src/widget/foldable.rs
	@! grep -RInE 'egui::Frame|Frame::new|Margin::symmetric' crates/core/src/command_palette.rs
	@! grep -RInE 'vertical_scroll_area_for_region|spacing_mut[(][)][.]item_spacing' crates/core/src/command_palette.rs
	@! grep -RInE '[.](request_focus|has_focus)[(]' crates/core/src/command_palette.rs
	@! grep -RInE 'show_singleline_text_edit_for_spec|request_focus_for_ui_response|has_focus' crates/core/src/command_palette.rs
	@! grep -RIn 'ui\.add_space' crates/core/src/command_palette.rs
	@! grep -RIn 'egui::Color32::from_rgba_unmultiplied' crates/core/src/command_palette.rs
	@! grep -RInE '(^|[^_])area_for_host' crates/core/src/command_palette.rs
	@! grep -RInE '^pub fn command_palette[(][^#]*egui::Context|pub use command_palette::.*command_palette' crates/core/src
	@! grep -RInE 'painter[(][)][.](add|set)|Shape::Noop|shape_from_paint_cmd' crates/core/src/command_palette.rs
	@! grep -RInE 'input_mut|egui::Key' crates/core/src/command_palette.rs
	@! grep -RInE 'consume_key[(]' crates/core/src/command_palette.rs
	@! grep -RIn 'ctx\.content_rect' crates/core/src/command_palette.rs
	@! grep -RIn 'ui\.ctx[(][)]' crates/core/src/command_palette.rs
	@! grep -RIn 'egui::Color32' crates/core/src/command_palette.rs
	@! grep -RInE 'color32_for_backend|accent_egui' crates/core/src/command_palette.rs
	@! grep -RInE 'egui::CursorIcon|ui\.ctx[(][)]|egui::Color32' crates/core/src/container/separator/mod.rs
	@! grep -RInE 'pub fn (paint_separator|paint_separator_resize)[(][^#]*&mut[[:space:]]+Ui' crates/core/src/container/separator/mod.rs
	@! sed -n '/pub fn paint/,/^    }/p' crates/core/src/container/body/mod.rs | grep -nE 'available_rect_before_wrap|allocate_ui_with_layout|ScrollArea::vertical|ui[.]add_space'
	@! grep -RIn 'egui::CursorIcon' crates/core/src/container/normal/mod.rs
	@! grep -RInE 'pub fn (show|show_tabs)[(][^#]*&mut[[:space:]]+Ui' crates/core/src/container/normal/mod.rs
	@! (awk '/^#\[cfg\(test\)\]/ { exit } { print }' crates/core/src/container/normal/mod.rs | grep -nE 'ui[.]interact[(]|ui[.]allocate_rect[(]|[.]allocate_exact_size[(]')
	@! (awk '/^#\[cfg\(test\)\]/ { exit } { print }' crates/core/src/container/normal/mod.rs | grep -nE 'ui[.]allocate_space[(]|ui[.]add_space[(]|child[.]add_space[(]|spacing_mut[(][)][.]item_spacing')
	@! (awk '/^#\[cfg\(test\)\]/ { exit } { print }' crates/core/src/container/normal/mod.rs | grep -nE 'UiBuilder|new_child[(]')
	@! (awk '/^#\[cfg\(test\)\]/ { exit } { print }' crates/core/src/container/normal/mod.rs | grep -nE 'egui::Direction|Layout::(top_down|bottom_up|left_to_right|right_to_left)|egui::Align::|[^[:alnum:]_]Align::')
	@! (awk '/^#\[cfg\(test\)\]/ { exit } { print }' crates/core/src/container/normal/mod.rs | grep -nE 'ui[.]available_rect_before_wrap[(]|fn tabbed_container_max_rect[(][^)]*egui::Rect|->[[:space:]]*egui::Rect[[:space:]]*[{]')
	@! (awk '/^#\[cfg\(test\)\]/ { exit } { print }' crates/core/src/container/normal/mod.rs | grep -nE 'ui[.]painter_at[(]|let[[:space:]]+painter[[:space:]]*=[[:space:]]*ui[.]painter[(][)]')
	@! (awk '/^#\[cfg\(test\)\]/ { exit } { print }' crates/core/src/container/normal/mod.rs | grep -nE 'render_paint_cmd[(]ui[.]painter[(][)]|render_paint_cmd[(][&]?ui[.]painter')
	@! (awk '/^#\[cfg\(test\)\]/ { exit } { print }' crates/core/src/container/normal/mod.rs | grep -nE 'render_paint_cmd_ui|render_paint_cmd[(]')
	@! (awk '/^#\[cfg\(test\)\]/ { exit } { print }' crates/core/src/container/normal/mod.rs | grep -nE 'Shape::Noop|shape_from_paint_cmd|ui[.]painter[(][)][.](add|set)|with_clip_rect[(]')
	@! (awk '/^#\[cfg\(test\)\]/ { exit } { print }' crates/core/src/container/normal/mod.rs | grep -nE 'ctx[(][)][.]layer_painter|crate::layer::painter|UiBuilder::new[(][)][.]layer_id|render_paint_cmd[(][&]p')
	@! (awk '/^#\[cfg\(test\)\]/ { exit } { print }' crates/core/src/container/normal/mod.rs | grep -nE 'ui[.]ctx[(][)][.](pointer_(interact|latest)_pos|input|request_repaint|animate_value_with_time)[(]')
	@! (awk '/^#\[cfg\(test\)\]/ { exit } { print }' crates/core/src/container/normal/mod.rs | grep -nE 'egui::pos2|egui::vec2|[[:space:]]pos2[(]|[[:space:]]vec2[(]')
	@! (sed -n '/let title_size/,/let body_cfg/p' crates/core/src/container/normal/mod.rs | grep -nE 'egui::Vec2|egui::vec2|[[:space:]]vec2[(]')
	@! (sed -n '/let render_body/,/body_cfg[.]paint/p' crates/core/src/container/normal/mod.rs | grep -nE 'egui::Rect::from_min|Rect::from_min_size|egui::pos2|egui::Vec2|egui::vec2|[[:space:]]pos2[(]|[[:space:]]vec2[(]')
	@! (sed -n '/fn show_inner_tabbed[(]/,/fn show_inner_tabbed_title_row/p' crates/core/src/container/normal/mod.rs | grep -nE 'egui::Rect::from_min|egui::pos2|egui::vec2|[[:space:]]pos2[(]|[[:space:]]vec2[(]')
	@! (sed -n '/fn show_inner_tabbed_title_row/,/fn show_inner[(]/p' crates/core/src/container/normal/mod.rs | grep -nE 'egui::Rect::from_min|egui::pos2|egui::vec2|[[:space:]]pos2[(]|[[:space:]]vec2[(]')
	@! (sed -n '/let sep_rect_after/,/pub(crate) fn show_raw/p' crates/core/src/container/normal/mod.rs | grep -nE 'egui::Rect::from_min|egui::pos2|egui::vec2|[[:space:]]pos2[(]|[[:space:]]vec2[(]')
	@! (sed -n '/let banner_cmd/,/paint_corner_ticks/p' crates/core/src/container/normal/mod.rs | grep -nE 'egui::Rect::from_min|egui::pos2|egui::vec2|[[:space:]]pos2[(]|[[:space:]]vec2[(]')
	@! (sed -n '/fn paint_folder_tabs/,/fn paint_top_tabs/p' crates/core/src/container/normal/mod.rs | grep -nE 'egui::Rect::from_min|egui::pos2|egui::vec2|[[:space:]]pos2[(]|[[:space:]]vec2[(]')
	@! (sed -n '/fn paint_top_tabs/,/fn paint_tab_rect_chrome/p' crates/core/src/container/normal/mod.rs | grep -nE 'egui::Rect::from_min|egui::pos2|egui::vec2|[[:space:]]pos2[(]|[[:space:]]vec2[(]')
	@! (sed -n '/fn paint_floating_icon/,/fn paint_cmd/p' crates/core/src/container/normal/mod.rs | grep -nE 'egui::Rect::from_min|Rect::from_min|egui::pos2|egui::vec2|[[:space:]]pos2[(]|[[:space:]]vec2[(]')
	@! grep -RInE 'pub fn new[(][^#]*id:[[:space:]]*impl[[:space:]]+std::hash::Hash' crates/core/src/container/tabbed/mod.rs
	@! grep -RInE 'egui::CursorIcon|on_hover_cursor|circle_filled' crates/core/src/pane/dots.rs
	@! grep -RInE 'allocate_exact_size|ui[.]interact[(]|egui::Sense|use egui::.*Response|pub fn paint_container_dots|ui[.]painter[(][)]' crates/core/src/pane/dots.rs
	@! grep -RInE 'pub use dots::paint_container_dots' crates/core/src/pane/mod.rs
	@! grep -RIn 'egui::CursorIcon' crates/core/src/pane/mod.rs
	@! grep -RInE 'egui::Area::new|egui::Order::' crates/core/src/pane/drag.rs crates/core/src/pane/tab_drag.rs
	@! (awk '/^#\[cfg\(test\)\]/ { exit } { print }' crates/core/src/pane/drag.rs | grep -nE 'allocate_exact_size|egui::Sense|egui::Rect::from_min_size|egui::pos2|ui[.]painter[(][)][.]rect|egui::Stroke::new|egui::StrokeKind')
	@! (awk '/^#\[cfg\(test\)\]/ { exit } { print }' crates/core/src/pane/tab_drag.rs | grep -nE 'button_size:[[:space:]]*egui::Vec2|egui::pos2|ui[.]painter[(][)][.]rect|egui::Stroke::new|egui::StrokeKind')
	@! grep -RInE 'ui[.]painter[(][)]|egui::FontId|egui::FontFamily|egui::epaint::TextShape|egui::Align2|egui::CornerRadius|ui[.]ctx[(][)][.](request_repaint|input)' crates/core/src/pane/title.rs
	@! (awk '/^#\[cfg\(test\)\]/ { exit } { print }' crates/core/src/embed.rs | grep -nE 'ui[.]painter[(][)]|render_paint_cmd[(]ui[.]painter')
	@! grep -RInE 'egui::CursorIcon|on_hover_cursor' crates/core/src/shelf
	@! grep -RInE 'egui::Area::new|egui::Order::' crates/core/src/shelf/mod.rs
	@! grep -RInE 'allocate_exact_size' crates/core/src/shelf/mod.rs
	@! (awk '/^#\[cfg\(test\)\]/ { exit } { print }' crates/core/src/shelf/mod.rs | grep -nE 'ui[.]interact[(]|egui::Sense|Sense::(drag|click_and_drag)[(]')
	@! (awk '/^#\[cfg\(test\)\]/ { exit } { print }' crates/core/src/shelf/mod.rs | grep -nE 'ui[.]painter[(][)]|render_paint_cmd[(]ui[.]painter')
	@! (awk '/^#\[cfg\(test\)\]/ { exit } { print }' crates/core/src/shelf/mod.rs | grep -nE 'area_for_host|show_area_for_host|ui[.]set_min_size|[.]request_repaint[(][)]')
	@! (awk '/^#\[cfg\(test\)\]/ { exit } { print }' crates/core/src/shelf/mod.rs | grep -nE 'backend::egui::(pointer_|primary_pointer)')
	@! (awk '/^#\[cfg\(test\)\]/ { exit } { print }' crates/core/src/shelf/mod.rs | grep -nE 'primary_pointer_down|pointer_(interact|latest)_pos[(]ctx|pointer_any_released[(]ctx')
	@! (awk '/^#\[cfg\(test\)\]/ { exit } { print }' crates/core/src/shelf/mod.rs | grep -nE 'egui::ScrollArea|spacing_mut[(][)][.]item_spacing')
	@! (awk '/^#\[cfg\(test\)\]/ { exit } { print }' crates/core/src/shelf/mod.rs | grep -nE 'child_ui_for_region|apply_scroll_region_spacing|scroll_area_for_region|show_sticky_scroll_area')
	@! (awk '/^#\[cfg\(test\)\]/ { exit } { print }' crates/core/src/shelf/mod.rs | grep -nE 'UiBuilder|new_child|egui::Layout|egui::Align')
	@! (awk '/^#\[cfg\(test\)\]/ { exit } { print }' crates/core/src/shelf/mod.rs | grep -nE '[.]pointer_(interact|latest)_pos[(][)]|ctx[.]input|viewport[.]ctx[(][)][.]input|key_pressed[(]egui::Key|pointer[.](any_released|primary_down)')
	@! grep -RInE 'egui::CursorIcon|on_hover_cursor' crates/core/src/embed.rs
	@! grep -RInE 'allocate_exact_size|allocate_rect|on_hover_text|hover_cursor_for_raw_response|egui::Sense::(hover|click|click_and_drag)' crates/core/src/embed.rs
	@! grep -RIn 'accent:[[:space:]]*egui::Color32' crates/core/src/embed.rs
	@! grep -RIn 'min_size:[[:space:]]*egui::Vec2' crates/core/src/embed.rs
	@! grep -RIn 'rect:[[:space:]]*egui::Rect' crates/core/src/embed.rs
	@! grep -RInE 'maximize_state_key[(].*[)] -> egui::Id|fullscreen_owner[(].*[)] -> Option<egui::Id>' crates/core/src/embed.rs
	@! grep -RInE '^pub fn (fullscreen_owner|is_any_fullscreen|set_fullscreen_minimize_chip_visible|restore_fullscreen)[(][^)]*egui::Context' crates/core/src/embed.rs
	@! grep -RInE 'mara_core::embed::(fullscreen_owner|is_any_fullscreen|set_fullscreen_minimize_chip_visible|restore_fullscreen)' example/src
	@! grep -RIn 'maximize_state_key(egui::Id' crates/core/src/extras/graph.rs
	@! grep -RInE 'pub fn is_(graph|code)_fullscreen[(][^#]*egui::Context' crates/core/src/extras/graph.rs crates/core/src/extras/code.rs
	@! grep -RInE '^[[:space:]]*accent:[[:space:]]*egui::Color32|^[[:space:]]*desired_size:[[:space:]]*egui::Vec2|Option<egui::Vec2>|insert_temp::<egui::Vec2>' crates/core/src/extras/graph.rs
	@! grep -RInE 'pub fn ctx[(]&self[)] -> &egui::Context|NodeViewState::ctx' crates/modules/graph/src/node_view.rs crates/core/src/extras/graph.rs
	@! grep -RInE '^[[:space:]]*accent:[[:space:]]*egui::Color32|^[[:space:]]*min_size:[[:space:]]*egui::Vec2' crates/core/src/extras/code.rs
	@! grep -RInE 'egui::(Pos2|Vec2|Color32|FontId|Align2|Stroke::new)|allocate_painter|interact_pointer_pos' crates/modules/canvas/src/lib.rs
	@! grep -RInE 'egui::(Pos2|Vec2|Color32|FontId|Align2|Stroke::new)|allocate_exact_size|painter_at|ui[.]painter|__internal_raw_ui' crates/modules/image/src/lib.rs
	@! grep -RInE 'prewarm_tiles[(].*egui::Vec2|__internal_raw_ui|mara_core::(readout|button)[(]' crates/modules/map/src/lib.rs
	@! grep -RInE '^[[:space:]]*pub [A-Za-z_][A-Za-z0-9_]*:[[:space:]]*egui::(Color32|Stroke)|pub fn egui_id[(]' crates/modules/map/src/lib.rs
	@! grep -RInE '^[[:space:]]*pub fn __internal_(prewarm_tiles|show)[(]' crates/modules/map/src/lib.rs
	@! grep -RInE '^pub fn (prewarm_tiles|show)[(][^#]*egui::Context|MaraMap::new[(].*[.]show[(][^)]*__internal_egui_ctx' crates/modules/map/src/lib.rs example/src
	@! grep -RInE 'MapIconGlyph::Fluent[(]name[)] => mara_core::icons::__internal_paint_section_icon_egui|MapIconGlyph::Svg[(]_+[)] => None|egui::FontId::proportional[(]icon[.]size[)]|__internal_paint_section_icon_egui' crates/modules/map/src/lib.rs
	@! sed -n '/fn paint_annotation/,/fn map_annotation_paint_cmds/p' crates/modules/map/src/lib.rs | grep -nE 'painter[.](circle_filled|circle_stroke|add)|egui::Shape::line|paint_polygon'
	@! sed -n '/fn paint_selected_feature/,/fn selection_color/p' crates/modules/map/src/lib.rs | grep -nE 'painter[.](circle_filled|circle_stroke|add)|egui::Shape::line|paint_polygon'
	@! awk '/fn paint_draft/,/^}/ { print }' crates/modules/map/src/lib.rs | grep -nE 'painter[.](circle_filled|circle_stroke|add)|egui::Shape::line|paint_polygon'
	@! awk '/fn paint_area_fill/,/^}/ { print }' crates/modules/map/src/mvt.rs | grep -nE 'painter[.](add|line_segment|rect_filled)|egui::Shape::line|paint_polygon'
	@! awk '/fn paint_building_extrusion/,/^}/ { print }' crates/modules/map/src/mvt.rs | grep -nE 'painter[.](add|line_segment)|egui::Shape::mesh'
	@! awk '/fn paint_feature_lines/,/^}/ { print }' crates/modules/map/src/mvt.rs | grep -nE 'painter[.](add|line_segment|rect_filled)|egui::Shape::line|paint_polygon'
	@! awk '/fn paint_label/,/^}/ { print }' crates/modules/map/src/mvt.rs | grep -nE 'painter[.]text'
	@! grep -RInE 'layout_no_wrap|egui::FontId|Vec<egui::Rect>' crates/modules/map/src/mvt.rs
	@! grep -RInE 'pub pointer_pos:[[:space:]]*Option<egui::Pos2>|pub scroll_delta:[[:space:]]*egui::Vec2|_position:[[:space:]]*egui::Pos2|__internal_raw_ui' crates/modules/three_d/src/lib.rs
	@! grep -RInE 'pub fn (from_response|allocate_viewport)[(][^#]*(egui::Response|egui::Ui)|pub type Color[[:space:]]*=[[:space:]]*egui::Color32' crates/modules/three_d/src/lib.rs
	@! grep -RInE 'BevyViewportPickedColor[(]pub Option<egui::Color32>|picked_color[(]&self[)] -> Option<egui::Color32>|accent:[[:space:]]*egui::Color32|[)] -> Option<egui::Color32>|pub fn show[(][^#]*egui::Context' crates/modules/bevy/src
	@! grep -RInE 'bevy_view[.]show[(]host[.]__internal_egui' example/src
	@! grep -RIn 'EmbeddedBevyViewport' crates/modules/bevy/src mara/plugin/bevy/src
	@! grep -nE '^pub type App[[:space:]]*=[[:space:]]*AppRunner' mara/src/window.rs
	@! grep -RIn 'Backwards-friendly' crates mara example
	@! grep -RInE 'pub fn show_app_shell.*accent:[[:space:]]*Color32|pub fn show_app_shell_.*accent:[[:space:]]*Color32|use egui::[{]Color32|use egui::Color32' crates/core/src/app_shell.rs
	@! grep -RInE '^pub fn show_app_shell|show_app_shell(_chrome_with_slot_ribbons|_with_slot_ribbons|_with_workspace_renderer)?,' crates/core/src/app_shell.rs crates/core/src/lib.rs
	@! grep -RInE 'pub fn draw_slot_ribbons[(][^)]*accent:[[:space:]]*Color32|pub fn draw_slot_ribbons_featureful[(][^)]*accent:[[:space:]]*Color32' crates/core/src/ribbon/slot_paint.rs
	@! grep -RInE 'pub fn draw_slot_ribbons[(]|pub fn draw_slot_ribbons_featureful[(]|draw_slot_ribbons,|draw_slot_ribbons_featureful,' crates/core/src/lib.rs crates/core/src/ribbon/mod.rs crates/core/src/ribbon/slot_paint.rs
	@! grep -RInE 'painter[.]rect|ui[.]painter[(][)][.]rect|egui::StrokeKind' crates/core/src/ribbon/paint.rs crates/core/src/ribbon/chrome.rs
	@! grep -RInE -- '->[[:space:]]*egui::Color32|fg:[[:space:]]*egui::Color32' crates/core/src/ribbon/paint.rs crates/core/src/ribbon/chrome.rs crates/core/src/ribbon/slot_paint.rs
	@! grep -RInE 'use egui::.*Color32|accent:[[:space:]]*(egui::)?Color32' crates/core/src/ribbon/paint.rs crates/core/src/ribbon/chrome.rs crates/core/src/ribbon/slot_paint.rs
	@! grep -RInE 'egui::Area::new|egui::Order::' crates/core/src/ribbon/chrome.rs crates/core/src/ribbon/slot_paint.rs
	@! grep -RInE 'allocate_exact_size|ui[.]interact[(]|on_hover_text|move_to_top' crates/core/src/ribbon/chrome.rs crates/core/src/ribbon/slot_paint.rs
	@! grep -RInE 'show_area_for_host[(]|ui[.]painter[(][)]' crates/core/src/ribbon/chrome.rs
	@! awk '/struct ButtonPlacement/,/^}/ { print }' crates/core/src/ribbon/chrome.rs | grep -nE 'egui::(Rect|Align2|Vec2)'
	@! grep -nE 'fn screen_rect[(].*->[[:space:]]*egui::Rect' crates/core/src/ribbon/chrome.rs
	@! grep -nE 'fn (ribbon_rect|strip_rect|cluster_region)[(].*->[[:space:]]*egui::Rect' crates/core/src/ribbon/chrome.rs
	@! awk '/fn strip_rect/,/^fn cluster_region/ { print }' crates/core/src/ribbon/chrome.rs | grep -nE 'egui::(Rect|Pos2|Vec2|pos2|vec2)'
	@! awk '/fn cluster_region/,/^struct ButtonPlacement/ { print }' crates/core/src/ribbon/chrome.rs | grep -nE 'egui::(Rect|Pos2|Vec2|pos2|vec2)'
	@! grep -nE 'cursor:[[:space:]]*Option<egui::Pos2>' crates/core/src/ribbon/chrome.rs
	@! (sed -n '1,/^#\[cfg(test)\]/p' crates/core/src/ribbon/chrome.rs | grep -nE 'egui::(Pos2|pos2|PointerButton)|ctx[.]input|pointer[.]interact_pos|[.]pointer_interact_pos')
	@! (sed -n '1,/^#\[cfg(test)\]/p' crates/core/src/ribbon/chrome.rs | grep -n 'ctx.content_rect')
	@! grep -RInE 'get_temp::<egui::Rect>[(]crate::ribbon::chrome::chrome_bounds_key|insert_temp[(][[:space:]]*crate::ribbon::chrome::chrome_bounds_key[(][)][[:space:]]*,[[:space:]]*egui::Rect|insert_temp[(][[:space:]]*chrome_bounds_key[(][)][[:space:]]*,[[:space:]]*egui::Rect' crates/core/src
	@! grep -RInE 'use egui::.*(Rect|Vec2|pos2|vec2)|ribbon_origin[(][^#]*egui::|ribbon_origin[(].*->[[:space:]]*egui::Pos2' crates/core/src/ribbon/slot_paint.rs
	@! grep -RInE 'ctx[.]input|viewport[(][)][.]maximized' crates/core/src/ribbon/slot_paint.rs
	@! grep -RIn 'ctx.content_rect' crates/core/src/ribbon/slot_paint.rs
	@! grep -RInE 'ui[.]set_min_size[(]|ui[.]painter[(][)]|show_area_for_host[(]' crates/core/src/ribbon/slot_paint.rs
	@! grep -RInE 'id:[[:space:]]*impl Into<egui::Id>|^[[:space:]]*pub fn .*->[[:space:]]*egui::Id' crates/core/src/container/tabbed/mod.rs
	@! grep -RInE 'pub fn (body_openness|user_flow|set_user_flow|user_span|set_user_span)[(][^)]*pane_id:[[:space:]]*Id\\b|pub fn active_pane_key|pub fn publish_ribbon_pane_ids[(]|pub fn publish_ribbon_pane_ids[(][^#]*Into<Id>|pub fn new[(][^#]*id:[[:space:]]*impl Into<Id>' crates/core/src/pane/mod.rs
	@! grep -RInE 'mara_core::pane::publish_ribbon_pane_ids|egui::Id::new[(]item[.]id[)]' example/src
	@! grep -RInE 'draw_unified_ribbons[(][[:space:]]*ctx|fn draw_unified_ribbons[(][^#]*ctx:[[:space:]]*&egui::Context|egui::Id::new[(]ACTION_|mara_core::ribbon::draw_slot_ribbons_featureful|host[.]__internal_egui[(][)]' example/src
	@! grep -RInE 'pub response:[[:space:]]*Option<.*Response|response:[[:space:]]*(Some|None)[(]?' crates/core/src/ribbon example/src
	@! grep -RInE 'pub fn (body_openness|user_flow|set_user_flow|user_span|set_user_span)[(]' crates/core/src/pane/mod.rs
	@! grep -RInE 'pub fn (toggle_body|body_open_touched_at|fold_version|container_min_widths|container_min_flows|published_container_cids|publish_container_cid|published_body_extra_flow|publish_body_extra_flow|published_ribbon_edges)|pub fn published_pane_rects[(].*->[[:space:]]*Vec<egui::Rect>' crates/core/src/pane/mod.rs
	@! grep -RInE 'fn publish_pane_rect[(][^#]*egui::Rect|get_temp::<egui::Rect>[(]clip_key[)]|insert_temp[(]clip_key,[[:space:]]*frame_response[.]response[.]rect' crates/core/src/pane/mod.rs
	@! (awk '/^mod tests/ { exit } { print }' crates/core/src/pane/mod.rs | grep -nE 'ctx[.]input|ctx[.]content_rect|[.]pointer_(interact|latest)_pos[(][)]')
	@! sed -n '/fn paint_resize_handles_inner/,/fn pane_main_resize_cursor/p' crates/core/src/pane/mod.rs | grep -nE 'egui::Rect::from_min_max|egui::pos2'
	@! sed -n '/fn paint_resize_handles_inner/,/fn pane_main_resize_cursor/p' crates/core/src/pane/mod.rs | grep -nE 'ui[.]interact[(]|Sense::click_and_drag|egui::Rect::from[(]'
	@! sed -n '/fn pane_main_resize_handle_rect/,/fn pane_main_resize_cursor/p' crates/core/src/pane/mod.rs | grep -nE 'egui::(Rect|Pos2|Vec2|pos2|vec2)'
	@! grep -RInE 'ui[.]painter[(][)][.](rect|rect_filled|line_segment)' crates/core/src/pane/mod.rs
	@! grep -RInE 'compute_pane_pos[(].*screen:[[:space:]]*egui::Rect|use egui::|egui::(Align|Align2|Pos2|Rect|Vec2|pos2|vec2)' crates/core/src/pane/layout.rs
	@! grep -RInE '^pub fn (published_pane_rects|clear_published_pane_rects)[(][^#]*egui::Context|Vec<egui::Rect>' crates/core/src/pane/mod.rs
	@! grep -RInE "pub fn show<'spec>[[:space:]]*[(][^#]*egui::Context|Pane::show|[.]show[(]ctx,[[:space:]]*[|]body" crates/core/src/pane/mod.rs example/src/app.rs
	@! grep -RInE 'pub fn order[(][^#]*egui::Order|order:[[:space:]]*egui::Order' crates/core/src/pane/mod.rs
	@! grep -RInE 'egui::Area::new|egui::Order::' crates/core/src/pane/mod.rs
	@! (awk '/^#\[cfg\(test\)\]/ { exit } { print }' crates/core/src/pane/mod.rs | grep -nE 'spacing_mut[(][)][.]item_spacing|add_space[(]|set_max_(width|height)[(]|allocate_exact_size[(]|ScrollArea::|show_sticky_scroll_area')
	@! grep -RInE 'pub fn (container_initial_flow|set_container_initial_flow|container_flow|record_container_intrinsic)[^#]*cid:[[:space:]]*Id\\b' crates/core/src/container/mod.rs
	@! grep -RInE 'pub fn (container_initial_flow|set_container_initial_flow|container_flow|set_container_flow|container_flow_bounds|record_container_intrinsic)' crates/core/src/container/mod.rs
	@! sed -n '/pub fn set_container_flow/,/^[)]/p' crates/core/src/container/mod.rs | grep -nE 'cid:[[:space:]]*Id\\b'
	@! grep -RInE 'pub fn (normal|tabbed|container_id|pane_id|search_query|temp_string|add_normal|add_tabbed|render)[^#]*((Into<Id>)|([^-]>[[:space:]]*Id\\b)|(HashMap<Id))' crates/core/src/pane/body.rs crates/core/src/shelf/mod.rs
	@! grep -RInE 'pub accent:[[:space:]]*Color32|pub fn new[(].*accent:[[:space:]]*Color32' crates/core/src/shelf/mod.rs
	@! grep -RInE 'pub id:[[:space:]]*Id|pub fn new[(]id:[[:space:]]*impl Into<Id>' crates/core/src/shelf/mod.rs
	@! awk '/pub struct ShelfLayout/,/^}/ { print }' crates/core/src/shelf/mod.rs | grep -nE ':[[:space:]]*(Option<)?Rect[>,]'
	@! grep -RInE 'pub fn layout_shelves[(][^#]*available:[[:space:]]*Rect|pub fn shelf_insets[(].*->[[:space:]]*Vec2' crates/core/src/shelf/mod.rs
	@! grep -RInE 'pub fn show_shelves|pub use shelf::.*show_shelves|responsive_shelves,[[:space:]]*shelf_insets,[[:space:]]*show_shelves' crates/core/src/shelf/mod.rs crates/core/src/lib.rs
	@! grep -RInE '^pub fn (publish_shelf_layout|shelf_layout|shelf_layout_published_this_pass)[(][^#]*egui::Context' crates/core/src/shelf/mod.rs
	@! grep -RInE 'pub use .*(publish_shelf_layout|shelf_layout|shelf_layout_published_this_pass)|mara_core::(publish_shelf_layout|shelf_layout|shelf_layout_published_this_pass)' crates/core/src/lib.rs example/src mara/plugin/bevy/src
	@! grep -RInE 'use egui::Color32|build_ribbon[(]&self,[[:space:]]*_accent' crates/core/src/shell.rs
	@! grep -RInE 'accent:[[:space:]]*egui::Color32|fill:[[:space:]]*egui::Color32|Option<egui::Color32>' crates/core/src/widget/button.rs crates/core/src/widget/chip.rs crates/core/src/widget/badge.rs crates/core/src/widget/progressbar.rs crates/core/src/widget/toggle.rs crates/core/src/widget/slider.rs crates/core/src/widget/dropdown.rs crates/core/src/widget/text_input/mod.rs crates/core/src/widget/select.rs crates/core/src/widget/color.rs crates/core/src/widget/foldable.rs crates/core/src/widget/context_menu.rs
	@! grep -RIn 'accent_egui' crates/core/src/widget/button.rs
	@! grep -RInE 'fn (lerp_col|lerp_col_alpha|with_alpha)[(].*egui::Color32' crates/core/src/widget/button.rs
	@! grep -RInE 'ui[.]ctx[(][)]|allocate_exact_size|painter_at|painter[(][)]|painter[.](rect_filled|rect_stroke)|egui::(Stroke::new|CornerRadius::same|Color32|lerp|Id)' crates/core/src/widget/button.rs
	@! grep -RInE 'egui::Color32|accent_egui' crates/core/src/widget/chip.rs
	@! grep -RInE 'egui::Color32|egui::Stroke|egui::lerp|egui::CornerRadius' crates/core/src/widget/progressbar.rs crates/core/src/widget/toggle.rs crates/core/src/widget/slider.rs
	@! grep -RInE 'egui::Color32|accent_egui' crates/core/src/widget/select.rs
	@! grep -RInE 'accent_egui|ui[.](id|ctx|add_space|available_width)[(]|egui::(Color32|color_picker)' crates/core/src/widget/color.rs
	@! grep -RInE 'ui[.](id|ctx)[(]|MaraMemoryCtx' crates/core/src/widget/foldable.rs
	@! grep -RInE 'egui::Color32|egui::Stroke|ui[.](ctx|id)[(]' crates/core/src/widget/dropdown.rs crates/core/src/widget/text_input/mod.rs
	@! grep -RInE 'pub fn context_menu_mara|spacing_mut[(][)][.]item_spacing' crates/core/src/widget/context_menu.rs
	@! grep -RInE '^pub fn (label|label_colored|readout|readout_h|chip|chip_colored|keybinding_row|keybinding_row_h|badge_row|badge_row_colored|button|button_h|card_button|card_action_button|progressbar|progressbar_h|toggle|toggle_h|toggle_track_only|slider|slider_h|drag_value|drag_value_h|axis_drag|axis_drag_h|select_row|select_row_h|hybrid_select_row|hybrid_select_row_h|dropdown|dropdown_h|color_rgb|color_rgba|text_input|text_input_h|section)[(]' crates/core/src/widget
	@! grep -nE 'pub fn card_button[(]|pub use .*card_button' crates/core/src/mui/mod.rs
	@! grep -RIn 'Compatibility shortcut' crates mara example
	@! grep -RInE 'pub fn show[(]self,[[:space:]]*ui:[[:space:]]*&mut[[:space:]]+egui::Ui' crates/core/src/widget/button.rs
	@! grep -RInE 'egui::FontId|LayoutJob|layout_job|painter[.](text|galley)' crates/core/src/widget/button.rs
	@! grep -RInE 'egui::FontId|TextShape|LayoutJob|layout_job|[.]galley[(]|painter[.](text|galley)|ui[.]fonts' crates/core/src/container/normal/mod.rs
	@! grep -RInE '^pub fn (tree_row|tree_action_row|tree_action_row_with_guide)[(]|pub fn new[(]ui:[[:space:]]*&.*mut[[:space:]]+egui::Ui' crates/core/src/widget/tree.rs
	@! grep -RInE 'egui::Rect::from_min|egui::pos2|egui::vec2|[[:space:]]pos2[(]|[[:space:]]vec2[(]|allocate_exact_size' crates/core/src/widget/tree.rs
	@! grep -RInE 'ui[.]interact[(]|egui::Sense|on_hover_text|Vec<egui::Response>|ui[.]ctx[(][)][.]input' crates/core/src/widget/tree.rs
	@! grep -RInE 'ui[.]painter|render_paint_cmd[(]|shape_from_paint_cmd|Shape::Noop|egui::Color32' crates/core/src/widget/tree.rs
	@! (awk '/Typed Pod tree builder/ { exit } { print }' crates/core/src/widget/tree.rs | grep -nE 'ui[.](ctx|id|clip_rect|available_width)[(]')
	@! grep -RInE 'pub fn ctx(_mut)?[(]|raw-egui escape hatch' crates/core/src/widget/tree.rs
	@! (sed -n '/pub(crate) fn tree_row[(]/,/pub(crate) fn tree_action_row[(]/p' crates/core/src/widget/tree.rs | grep -nE 'egui::Rect::from_min|egui::pos2|egui::vec2|[[:space:]]pos2[(]|[[:space:]]vec2[(]|allocate_exact_size')
	@! (sed -n '/fn compute_row_rects/,/fn tree_indent_guide_paint_cmds/p' crates/core/src/widget/tree.rs | grep -nE 'egui::Rect::from_min|egui::pos2|egui::vec2|[[:space:]]pos2[(]|[[:space:]]vec2[(]|allocate_exact_size')
	@! grep -RInE 'mara_core::widget::(drag_value|toggle|slider|dropdown)::(drag_value|toggle|slider|dropdown)' example/src
	@! grep -nE 'accent:[[:space:]]*egui::Color32|from_raw[(].*accent:[[:space:]]*impl Into<egui::Color32>|__internal_from_raw[(].*accent:[[:space:]]*impl Into<egui::Color32>' crates/core/src/mui/mod.rs
	@! grep -nE 'self[.]ui[.](id|available_width|available_height|available_rect_before_wrap|add_space|interact|painter_at|clip_rect)[(]' crates/core/src/mui/mod.rs
	@! grep -nE 'self[.]ui[.]ctx[(][)]|with_response[(]self[.]ui[.]ctx' crates/core/src/mui/mod.rs
	@! grep -nE '[.](horizontal|vertical)[(]' crates/core/src/mui/mod.rs
	@! grep -nE 'show_(horizontal|vertical)_for_ui' crates/core/src/mui/mod.rs crates/core/src/backend/egui.rs
	@! grep -nE 'painter_for_ui_available_rect' crates/core/src/mui/mod.rs crates/core/src/backend/egui.rs
	@! grep -nE 'mara_label[(]self[.]ui|readout(_h)?[(]self[.]ui' crates/core/src/mui/mod.rs
	@! grep -nE 'readout[(]ui,' crates/core/src/pod/mod.rs
	@! grep -nE 'chip(_colored)?[(]self[.]ui' crates/core/src/mui/mod.rs
	@! grep -nE 'chip(_colored)?[(]ui,' crates/core/src/pod/mod.rs
	@! grep -RInE 'pub[(]crate[)][[:space:]]+fn[[:space:]]+(label|label_colored|readout|readout_h)[(][^#]*egui::Ui' crates/core/src/widget/label.rs crates/core/src/widget/readout.rs
	@! grep -RInE 'pub[(]crate[)][[:space:]]+fn[[:space:]]+chip(_colored)?[(][^#]*egui::Ui' crates/core/src/widget/chip.rs
	@! grep -nE 'keybinding_row(_h)?[(]self[.]ui' crates/core/src/mui/mod.rs
	@! grep -nE 'keybinding_row_h[(]ui,' crates/core/src/pod/mod.rs
	@! grep -RInE 'pub[(]crate[)][[:space:]]+fn[[:space:]]+keybinding_row(_h)?[(][^#]*egui::Ui' crates/core/src/widget/keybinding.rs
	@! grep -nE 'badge_row(_colored)?[(]self[.]ui' crates/core/src/mui/mod.rs
	@! grep -nE 'badge_row_colored[(]ui,' crates/core/src/pod/mod.rs
	@! grep -RInE 'pub[(]crate[)][[:space:]]+fn[[:space:]]+badge_row(_colored)?[(][^#]*egui::Ui' crates/core/src/widget/badge.rs
	@! grep -nE 'progressbar(_h)?[(]self[.]ui' crates/core/src/mui/mod.rs
	@! grep -nE 'progressbar[(]ui,' crates/core/src/pod/mod.rs
	@! grep -RInE 'pub[(]crate[)][[:space:]]+fn[[:space:]]+progressbar(_h)?[(][^#]*egui::Ui' crates/core/src/widget/progressbar.rs
	@! grep -nE 'toggle(_track_only)?[(]self[.]ui' crates/core/src/mui/mod.rs
	@! grep -nE 'toggle[(]ui,' crates/core/src/pod/mod.rs
	@! grep -RInE 'pub[(]crate[)][[:space:]]+fn[[:space:]]+toggle(_h|_track_only)?[(][^#]*egui::Ui' crates/core/src/widget/toggle.rs
	@! grep -nE 'self[.]painter[(][)][.]line_segment' crates/core/src/mui/mod.rs
	@! grep -nE 'interact_for_ui_rect[(]self[.]ui|painter_for_ui_(rect|clip)[(]self[.]ui' crates/core/src/mui/mod.rs
	@! grep -nE 'pub[(]crate[)][[:space:]]+ui:[[:space:]]*&' crates/core/src/mui/mod.rs
	@! grep -RInE 'pub accent:[[:space:]]*Color32|accent:[[:space:]]*Color32|use egui::Color32|use egui::[{]Color32' crates/core/src/view/context.rs crates/core/src/module/context.rs
	@! grep -RInE 'pub fn new[(][^#]*egui::Context|ViewCtx::new[(]' crates/core/src/view/context.rs crates mara example
	@! grep -RInE 'egui::Painter::new|egui::LayerId|egui::Area::new|egui::Order::' crates/core/src/view/context.rs
	@! grep -RInE 'use egui::Id|pub [A-Za-z_][A-Za-z0-9_]*:[[:space:]]*Id\\b|impl Into<Id>|push_module[(][^)]*:[[:space:]]*Id\\b|push_module_workspace[(][^)]*:[[:space:]]*Id\\b|pub pod_id:[[:space:]]*Id\\b' crates/core/src/workspace crates/core/src/module/context.rs
	@! grep -RInE 'use egui::Id|pub .*egui::Id|RibbonAction::Command[(]egui::Id::new|RibbonAction::PushModuleWorkspace[(]egui::Id::new|RibbonScope::WorkspaceLevel[(]egui::Id::new' crates/core/src/ribbon/action.rs crates/core/src/ribbon/slot.rs crates/core/src/ribbon/dispatch.rs crates/core/src/ribbon/permanent.rs crates/core/src/app_shell.rs crates/core/src/shell.rs crates/core/tests/ribbon_slots.rs crates/core/tests/app_shell.rs example/src/app.rs
	@! grep -RInE 'pub id:[[:space:]]*Id|pub fn new[(]id:[[:space:]]*impl Into<Id>|pub fn (widget_height_key|search_query|forced_height_key|id)[^#]*([,(][[:space:]]*[A-Za-z_]*:?[[:space:]]*Id\\b|->[[:space:]]*Id\\b)' crates/core/src/pod/mod.rs
	@! grep -RInE 'pub fn show[(]self,[[:space:]]*ui:[[:space:]]*&mut[[:space:]]+(Ui|egui::Ui)' crates/core/src/pod/mod.rs
	@! grep -RInE 'pub fn with_.*accent:[[:space:]]*Color32' crates/core/src/pod/mod.rs
	@! grep -RInE 'pub fn (appearance_session|scramble_text|scramble_active|glitch_text|chromatic_aberration_offset|set_screen_metrics)[(][^#]*egui::Context' crates/core/src/style.rs
	@! grep -RInE '^pub fn (install_fonts|apply_theme|apply_theme_to)[(][^#]*egui::Context|^pub fn (install_fonts|apply_theme|apply_theme_to)[(]' crates/core/src/style.rs
	@! grep -RInE 'pub use .*(install_fonts|apply_theme|apply_theme_to)|mara_core::style::(install_fonts|apply_theme|apply_theme_to)|mara_core::(install_fonts|apply_theme|apply_theme_to)' crates/core/src/lib.rs mara/plugin/bevy/src example/src
	@! grep -RInE 'pub struct AccentColor[(]pub egui::Color32|pub const ACCENT_NEUTRAL:[[:space:]]*egui::Color32|pub fn srgb_to_egui|set_accent_color[(][^#]*egui::Color32' crates/core/src/style.rs example/src/app.rs
	@! grep -RInE 'pub fn (active_accent|raw_accent)[(][)] -> egui::Color32' crates/core/src/style.rs
	@! grep -RInE 'pub fn (fg_dim|on_panel|on_panel_dim|on_section|on_section_dim|on_track|on_track_dim|accent_hover|accent_pressed)[(][)] -> egui::Color32' crates/core/src/style.rs
	@! grep -RInE 'pub fn contrast_text_for[(].*->[[:space:]]*egui::Color32' crates/core/src/style.rs
	@! grep -RInE 'pub fn section_title_color[(].*->[[:space:]]*egui::Color32' crates/core/src/style.rs
	@! grep -RInE 'pub fn (outline_base|widget_border)[(].*->[[:space:]]*egui::Color32|pub fn widget_border[(][^#]*accent:[[:space:]]*egui::Color32' crates/core/src/style.rs
	@! grep -RInE 'pub fn (glass_fill|fill_for|body_accent|pane_fill|section_fill|subsection_fill|surface_lift_target|track_fill|popup_fill|row_hover_fill|row_selected_fill)[(].*->[[:space:]]*egui::Color32|pub fn (glass_fill|fill_for)[(][^#]*egui::Color32|pub fn row_alt_fill[(].*->[[:space:]]*Option<egui::Color32>' crates/core/src/style.rs
	@! grep -RInE 'pub fn stroke_for[(].*->[[:space:]]*egui::Stroke|pub fn stroke_for[(][^#]*accent:[[:space:]]*egui::Color32' crates/core/src/style.rs
	@! grep -RInE 'pub fn radius_for[(].*->[[:space:]]*egui::CornerRadius' crates/core/src/style.rs
	@! grep -RInE 'pub fn (high_contrast_accent|adapt_accent_to_mode)[(].*->[[:space:]]*egui::Color32|pub fn (high_contrast_accent|adapt_accent_to_mode)[(][^#]*accent:[[:space:]]*egui::Color32' crates/core/src/style.rs
	@! grep -RInE 'pub fn frame_for[(].*->[[:space:]]*egui::Frame|pub fn frame_for[(][^#]*accent:[[:space:]]*egui::Color32' crates/core/src/style.rs
	@! grep -RInE 'pub fn section_padding[(].*->[[:space:]]*egui::Margin' crates/core/src/style.rs
	@! grep -RInE 'pub fn (section_caps|title_text|body_label|caption)[(].*->[[:space:]]*egui::RichText|pub fn section_caps[(][^#]*accent:[[:space:]]*egui::Color32' crates/core/src/style.rs
	@! grep -RInE 'pub fn title_font_family[(].*->[[:space:]]*egui::FontFamily' crates/core/src/style.rs
	@! grep -RInE 'pub fn paint_caution_stripes|pub fn caution_stripes_paint_cmd[(][^#]*(egui::Painter|egui::Rect|egui::Color32)' crates/core/src/style.rs
	@! grep -RInE 'pub fn (paint_dashed_line|divider|thin_divider)[(][^#]*(egui::Painter|egui::Ui|egui::Pos2|egui::Stroke)' crates/core/src/style.rs
	@! grep -RInE '^pub fn (order_for|layer_id|painter)[(]' crates/core/src/layer.rs
	@! grep -RInE 'pub fn new[(][^#]*accent:[[:space:]]*Color32' crates/core/src/pane/mod.rs
	@! grep -RInE 'pub fn (icon|icon_text)[(].*egui::|pub fn icon_text|pub fn (paint_icon|paint_section_icon)[(]' crates/core/src/icons.rs
	@! grep -RInE '(^|[^_])(paint_icon|paint_section_icon)[(]' crates/core/src crates/modules example/src
	@! grep -RIn '__internal_paint_section_icon_egui' crates mara example
	@! grep -RIn '__internal_paint_icon_egui' crates mara example
	@! grep -RInE '^pub const .*egui::Color32' crates/core/src/style.rs crates/core/src/themes
	@! grep -nE 'pub use .*_(BG|BORDER)_' crates/core/src/style.rs
	@! grep -n 'pub mod debug' crates/core/src/lib.rs
	@! grep -RInE 'pub fn (claim_window_chrome_input|window_chrome_input_claimed|clear_window_chrome_regions)[(]|pub use .*claim_window_chrome_input|pub use .*window_chrome_input_claimed|pub use .*clear_window_chrome_regions' crates/core/src/window_chrome.rs crates/core/src/lib.rs
	@! grep -RInE '^pub fn (publish_window_chrome_regions|window_chrome_regions|publish_window_chrome_host_capabilities|window_chrome_host_capabilities|hit_test_window_chrome|hovered_resize_corner|paint_resize_corner_hover)[(][^#]*egui::Context' crates/core/src/window_chrome.rs
	@! grep -RInE 'pub use .*(publish_window_chrome_regions|window_chrome_regions|publish_window_chrome_host_capabilities|window_chrome_host_capabilities|hit_test_window_chrome|hovered_resize_corner|paint_resize_corner_hover)' crates/core/src/lib.rs
	@! grep -RInE 'drag_regions:[[:space:]]*Vec<egui::Rect>|exclusion_rects:[[:space:]]*Vec<egui::Rect>|pointer_pos:[[:space:]]*Option<egui::Pos2>|window_size:[[:space:]]*egui::Vec2' crates/core/src/window_chrome.rs
	@! grep -RInE 'fn (content_rect|screen_rect|ribbon_avoiding_rect)[(].*[)] -> egui::Rect' crates/core/src/view/context.rs
	@! grep -RInE 'pub fn (ribbon_avoiding_rect|main_bar_empty_drag_started)[(][^#]*egui::Context|pub fn apply_to_rect[(][^#]*egui::Rect' crates/core/src/ribbon/chrome.rs crates/core/src/module/context.rs
	@! grep -RInE 'ribbon_avoiding_rect|main_bar_empty_drag_started' crates/core/src/lib.rs
	@! awk '/pub struct MaraPainter/,/^}/ { print }' crates/core/src/mui/mod.rs | grep -n 'egui::Painter'
	@! grep -RIn 'ctx\.content_rect' crates/core/src/embed.rs
	@! grep -RIn 'pos:[[:space:]]*egui::Pos2' crates/core/src/embed.rs
	@! grep -RInE 'screen:[[:space:]]*egui::Rect|cursor:[[:space:]]*egui::Pos2|->[[:space:]]*egui::Pos2' crates/core/src/embed.rs
	@! grep -RInE 'Option<egui::Pos2>|remove::<egui::Pos2>|[.]pointer_interact_pos' crates/core/src/embed.rs
	@! grep -RInE 'egui::Area::new|egui::Order::|egui::LayerId|egui::Painter::new' crates/core/src/embed.rs
	@! (grep -RInE 'egui::Area::new|egui::Order::(Tooltip|Foreground|Middle)|egui::LayerId::new|egui::Painter::new' crates/core/src | grep -v 'crates/core/src/backend/egui.rs')
	@! grep -RInE '(^|[^_])painter[.]rect[(]' crates/core/src/embed.rs
	@! grep -RIn 'ghost_painter\.rect' crates/core/src/embed.rs
	@! grep -RInE 'ui\.painter[(][)][.](text|rect_filled)' crates/core/src/embed.rs
	@! grep -RInE 'line_segment|convex_polygon|painter[.]add' crates/core/src/embed.rs
	@! grep -RIn 'rect_filled(rect, 0.0, fill)' crates/core/src/shelf/mod.rs
	@! grep -RInE 'ui\.painter[(][)][.](rect|rect_filled|line_segment)' crates/core/src/shelf/mod.rs

harden:
	@git diff --check
	@$(CARGO) fmt --all -- --check
	@$(CARGO) check --workspace --no-default-features
	@$(CARGO) test --workspace --no-default-features
	@$(CARGO) clippy --workspace --all-targets --all-features -- -D warnings
	@$(CARGO) test --workspace --all-targets --all-features

bench:
	@$(CARGO) bench

docs:
	@command -v mdbook >/dev/null 2>&1 || { echo "mdbook is not installed. Please install it first."; exit 1; }
	@mdbook build $(TOP_DIR)/book --dest-dir $(TOP_DIR)/docs
	@git add --all && git commit -m "docs: building website/mdbook"

release:
	@if [ -z "$(HAS_REL)" ]; then \
		echo "git-rel is not installed. Please install it first."; \
		exit 1; \
	fi
	@if [ -z "$(TYPE)" ]; then \
		echo "Release type not specified. Use 'make release TYPE=[patch|minor|major|m.m.p]'"; \
		exit 1; \
	fi
	@git rel $(TYPE)

clean:
	@$(CARGO) clean

help:
	@echo
	@echo "Usage: make [target]"
	@echo
	@echo "Available targets:"
	@echo "  build        Build TARGET=native (default), TARGET=web, or TARGET=apk"
	@echo "  compile      Clean and rebuild"
	@echo "  run          Run the root $(APP_PKG) $(APP_BIN) app ($(BACKEND) backend, $(RUN_WITH) wrapper)"
	@echo "  serve        Build TARGET=web, then serve the root example UI in a browser"
	@echo "  test         Test the same app target as build/run ($(APP_PKG) bin $(APP_BIN))"
	@echo "  test-all     Run the full workspace all-target test suite"
	@echo "  check        Check the full workspace all-target suite"
	@echo "  harden       Run diff whitespace check + fmt/check + strict clippy + all-feature tests"
	@echo "  bench        Run benchmarks"
	@echo "  docs         Build documentation with mdbook"
	@echo "  release      Create a new release (TYPE=patch|minor|major|m.m.p)"
	@echo "  clean        Remove Cargo build artifacts"
	@echo
	@echo "Examples:"
	@echo "  make run"
	@echo "  make build TARGET=native"
	@echo "  make build TARGET=web"
	@echo "  make build TARGET=apk        # reserved for Android"
	@echo "  make serve"
	@echo "  make run APP_BIN=native       # run a different root example binary"
	@echo "  make run APP_BIN=bevy         # run the Bevy-owned Mara example"
	@echo "  make run BACKEND=x11          # force X11 / XWayland (.envrc auto-detects)"
	@echo "  make run BACKEND=wayland      # force native Wayland"
	@echo "  make run DISPLAY=:0           # target a different X server (BACKEND=x11)"
	@echo "  make run RUN_WITH=nixGL       # OpenGL wrapper instead of Vulkan"
	@echo "  make run RUN_WITH=            # no wrapper (native run)"
	@echo

h: help
