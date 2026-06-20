//! GAME theme — square corners, accent-tinted panels, bracket
//! titles on a solid accent banner, dashed row separators, L-bracket
//! corner ticks.
//!
//! See [`crate::themes`](super) for the "how to add a theme" guide.

use crate::style::{
    AXIS_X,
    AXIS_Y,
    AXIS_Z,
    ActiveIndicatorTheme,
    BadgeTheme,
    ButtonTheme,
    ChipTheme,
    CodeTheme,
    ColorMode,
    ColorTheme,
    ContainerTheme,
    DragValueTheme,
    DropdownTheme,
    GlassTheme,
    GraphCanvasPattern,
    GraphTheme,
    IconTheme,
    KeybindingTheme,
    Mode,
    ModuleTheme,
    MotionTheme,
    OverlayTheme,
    PaletteTheme,
    PaneTheme,
    PodTheme,
    ProgressTheme,
    ReadoutTheme,
    RibbonChromeTheme,
    RibbonTheme,
    SUCCESS,
    SelectTheme,
    ShapeTheme,
    ShelfTheme,
    SliderTheme,
    StrokeTheme,
    // shared text constants
    TEXT_DISABLED,
    TEXT_DISABLED_LIGHT,
    TEXT_PRIMARY,
    TEXT_PRIMARY_LIGHT,
    TEXT_SECONDARY,
    TEXT_SECONDARY_LIGHT,
    TabInactiveGlyphColor,
    TabLayout,
    TabOuterInset,
    TabTheme,
    TextColorMode,
    TextTheme,
    Theme,
    ThemeId,
    ToggleTheme,
    TreeTheme,
    ViewSwitcherLayout,
    ViewTheme,
    WidgetTheme,
    WindowChromeTheme,
};

/// GAME Light surface palette — bright accent-tinted surfaces, dark
/// text. Text colours flow through the shared `TEXT_*_LIGHT`
/// constants, not per-theme overrides.
pub(crate) const GAME_LIGHT_BG_WINDOW: crate::vocab::Color32 =
    crate::vocab::Color32::from_rgb(0xF0, 0xF1, 0xF5);
pub(crate) const GAME_LIGHT_BG_PANEL: crate::vocab::Color32 =
    crate::vocab::Color32::from_rgb(0xFA, 0xFB, 0xFD);
// Raised + input tightened (same reasoning as PRO Light). Raised
// flipped from `FFFFFF` (which was actually *brighter* than the
// panel — wrong direction for a Light theme) to a tone visibly
// darker than the panel. Input also pulled away from the panel.
pub(crate) const GAME_LIGHT_BG_RAISED: crate::vocab::Color32 =
    crate::vocab::Color32::from_rgb(0xF1, 0xF3, 0xF7);
pub(crate) const GAME_LIGHT_BG_HOVER: crate::vocab::Color32 =
    crate::vocab::Color32::from_rgb(0xE6, 0xE8, 0xEE);
pub(crate) const GAME_LIGHT_BG_INPUT: crate::vocab::Color32 =
    crate::vocab::Color32::from_rgb(0xEE, 0xF0, 0xF5);

/// Built-in GAME profile — square corners, accent-tinted panels,
/// bracket-decorated titles on a solid accent banner, dashed row
/// separators, L-bracket corner ticks. Pick a [`Mode`] to flip the
/// whole brightness axis: Dark lerps surfaces toward black for the
/// deep tactical look, Light lerps toward white for a paper /
/// accent-stained variant.
pub fn theme_game(mode: Mode) -> Theme {
    let dark = matches!(mode, Mode::Dark);
    let lerp_target = if dark {
        egui::Color32::BLACK
    } else {
        egui::Color32::WHITE
    };
    let lerp_factor = if dark { 0.22 } else { 0.18 };
    Theme {
        id: ThemeId {
            family: "GAME",
            variant: if dark { "DARK" } else { "LIGHT" },
        },
        name: if dark { "GAME_DARK" } else { "GAME_LIGHT" },
        is_light: !dark,
        palette: PaletteTheme {
            bg_window: if dark {
                egui::Color32::from_rgb(0x08, 0x0A, 0x12)
            } else {
                GAME_LIGHT_BG_WINDOW.into()
            },
            bg_panel: if dark {
                egui::Color32::from_rgb(0x10, 0x14, 0x1F)
            } else {
                GAME_LIGHT_BG_PANEL.into()
            },
            bg_raised: if dark {
                egui::Color32::from_rgb(0x16, 0x1B, 0x29)
            } else {
                GAME_LIGHT_BG_RAISED.into()
            },
            bg_hover: if dark {
                egui::Color32::from_rgb(0x1F, 0x26, 0x38)
            } else {
                GAME_LIGHT_BG_HOVER.into()
            },
            bg_input: if dark {
                egui::Color32::from_rgb(0x06, 0x08, 0x0E)
            } else {
                GAME_LIGHT_BG_INPUT.into()
            },
            text_primary: (if dark {
                TEXT_PRIMARY
            } else {
                TEXT_PRIMARY_LIGHT
            })
            .into(),
            text_secondary: (if dark {
                TEXT_SECONDARY
            } else {
                TEXT_SECONDARY_LIGHT
            })
            .into(),
            text_disabled: (if dark {
                TEXT_DISABLED
            } else {
                TEXT_DISABLED_LIGHT
            })
            .into(),
            border_subtle: if dark {
                egui::Color32::from_rgb(0x80, 0x80, 0x80)
            } else {
                egui::Color32::from_rgb(0x6B, 0x70, 0x78)
            },
            border_inner: egui::Color32::from_rgb(0x1F, 0x26, 0x38),
        },
        stroke: StrokeTheme {
            border_alpha: if dark { 35 } else { 0 },
            border_accent_tint: 0.0,
            border_width: if dark { 0.63 } else { 0.0 },
            row_separator_alpha: if dark { 25 } else { 28 },
            row_separator_dash: Some((4.0, 3.0)),
        },
        glass: GlassTheme {
            card_factor: 1.0,
            group_factor: 1.0,
            accent_tint: 0.0,
        },
        shape: ShapeTheme {
            radius_widget: 0,
            radius_compact: 0,
            radius_sm: 0,
            radius_md: 0,
            radius_lg: 0,
        },
        text: TextTheme {
            title_color_mode: TextColorMode::Accent,
            title_softness: 0.0,
            body_accent_darken: 0.18,
        },
        motion: MotionTheme {
            animations_enabled: true,
            button_anim_scale: 2.0,
            pane_fade_scale: 1.0,
            scramble_titles: true,
        },
        graph: GraphTheme {
            node_pad_x: 8,
            node_pad_y: 4,
            bg_inner_margin: 2,
            canvas_pattern: GraphCanvasPattern::Hex { radius: 24.0 },
            grid_alpha: 28,
            pin_stroke_width: 1.0,
            pin_stroke_alpha: 160,
            wire_width: 2.0,
            wire_glow: 1.0,
            pin_glow: 0.85,
            node_halo_gap: 3.0,
            node_halo_width: 1.5,
            node_halo_radius_outset: 3,
        },
        code: CodeTheme {
            font_size: 13.0,
            line_height_factor: 1.2,
            min_rows: 6,
            force_dark: true,
            functions: AXIS_Y.into(),
            literals: AXIS_X.into(),
            numerics: AXIS_X.into(),
            strings: SUCCESS.into(),
            types: AXIS_Z.into(),
        },
        overlay: OverlayTheme {
            inline_chip_size: 24.0,
            inline_chip_pad: 4.0,
            fullscreen_button_size: 34.0,
            fullscreen_edge_gap: 8.0,
            placeholder_text: "(maximised)",
            ghost_fill_alpha: 48,
            ghost_stroke_width: 1.5,
        },
        views: ViewTheme {
            switcher_layout: ViewSwitcherLayout::VerticalRail,
            switcher_button_min: 36.0,
            active_indicator_thickness: 3.0,
            active_indicator_inset: 0.0,
            close_icon: "dismiss",
        },
        window_chrome: WindowChromeTheme {
            resize_corner_extent: 30.0,
            resize_corner_edge_width: 4.8,
        },
        modules: ModuleTheme {
            allow_fullscreen_by_default: true,
            workspace_restore_icon: "arrow-minimize",
            workspace_restore_label: "Restore",
            inline_workspace_button_label: "Open workspace",
        },
        bg_window: if dark {
            egui::Color32::from_rgb(0x08, 0x0A, 0x12)
        } else {
            GAME_LIGHT_BG_WINDOW.into()
        },
        bg_panel: if dark {
            egui::Color32::from_rgb(0x10, 0x14, 0x1F)
        } else {
            GAME_LIGHT_BG_PANEL.into()
        },
        bg_raised: if dark {
            egui::Color32::from_rgb(0x16, 0x1B, 0x29)
        } else {
            GAME_LIGHT_BG_RAISED.into()
        },
        bg_hover: if dark {
            egui::Color32::from_rgb(0x1F, 0x26, 0x38)
        } else {
            GAME_LIGHT_BG_HOVER.into()
        },
        bg_input: if dark {
            egui::Color32::from_rgb(0x06, 0x08, 0x0E)
        } else {
            GAME_LIGHT_BG_INPUT.into()
        },
        panel_fill_mode: ColorMode::FromAccent {
            lerp_factor,
            lerp_target,
        },
        section_fill_mode: ColorMode::FromAccent {
            lerp_factor,
            lerp_target,
        },
        section_show_frame: true,
        section_show_title_divider: false,
        section_pad_x: 3,
        section_pad_y: 2,
        section_body_indent: 8.0,
        section_outer_margin_flow_title: 6,
        section_outer_margin_flow_body: 0,
        section_outer_margin_span: 1,
        section_body_inner_top_pad: 12.0,
        pane_title_chromatic_aberration: true,
        section_animation_time: 0.35,
        animations_enabled: true,
        button_anim_scale: 2.0,
        pane_fade_scale: 1.0,
        text_primary: (if dark {
            TEXT_PRIMARY
        } else {
            TEXT_PRIMARY_LIGHT
        })
        .into(),
        text_secondary: (if dark {
            TEXT_SECONDARY
        } else {
            TEXT_SECONDARY_LIGHT
        })
        .into(),
        text_disabled: (if dark {
            TEXT_DISABLED
        } else {
            TEXT_DISABLED_LIGHT
        })
        .into(),
        title_color_mode: TextColorMode::Accent,
        title_softness: 0.0,
        ribbon_button_accent_fill: true,
        section_gap: 4.0,
        section_corner_ticks_inset: 3.0,
        section_title_brackets: true,
        section_title_prefix: None,
        section_title_letter_spacing: 1.5,
        section_bottom_rule: true,
        pane_fill_visible: false,
        show_section_chevron: false,
        title_strip_filled: true,
        section_title_size: 11.5,
        body_accent_darken: 0.18,
        section_icon_at_end: true,
        section_icon_size: 20.0,
        section_body_top_pad: 16.0,
        row_separator_dash: Some((4.0, 3.0)),
        section_title_trailing_rule: false,
        section_corner_ticks: 10.0,
        tabs: TabTheme {
            layout: TabLayout::TitleRowSegmented,
            outer_inset: TabOuterInset::None,
            strip_thickness: 24.0,
            tab_len: 28.0,
            tab_gap: 6.0,
            tab_overlap: 2.5,
            title_row_height_multiplier: 2.0,
            folder_icon_size: 20.0,
            folder_active_radius: 0,
            inactive_glyph_color: TabInactiveGlyphColor::HighContrast,
        },
        pod: PodTheme {
            widget_spacing: 4.0,
            min_widget_h: crate::style::UNIT,
            max_widget_h: 240.0,
            tag_row_pitch: 20.0,
        },
        widgets: WidgetTheme {
            button: ButtonTheme {
                row_h: crate::widget::button::BUTTON_ROW_H,
                subtitle_row_h: crate::widget::button::BUTTON_ROW_H_SUBTITLE,
                label_font: crate::widget::button::BUTTON_LABEL_FONT,
                subtitle_font: crate::widget::button::BUTTON_SUBTITLE_FONT,
                glyph_font: crate::widget::button::BUTTON_GLYPH_FONT,
                edge_pad: 8.0,
                glyph_w: 14.0,
                glyph_gap: 8.0,
                full_accent_on_press: true,
                tint_rest: 0.12,
                tint_hover: 0.18,
                tint_press: 0.40,
            },
            progress: ProgressTheme {
                row_h: crate::widget::progressbar::PROGRESSBAR_ROW_H,
                value_font: crate::widget::progressbar::PROGRESSBAR_VALUE_FONT,
                segmented: true,
                segments: 12,
                segment_gap: 1.5,
                segment_inset: 1.5,
                dim_alpha: 38,
            },
            tree: TreeTheme {
                row_h: crate::widget::tree::TREE_ROW_H,
                indent: crate::widget::tree::TREE_INDENT,
                guide_width: 0.0,
                label_font: 12.0,
                chevron_w: 12.0,
                icon_w: 14.0,
                label_pad_l: 4.0,
                slot_w: 16.0,
                slot_gap: 2.0,
                right_pad_r: 4.0,
                row_pad_l: 4.0,
            },
            select: SelectTheme {
                row_h: 20.0,
                label_pad_l: 10.0,
                trailing_pad_r: 6.0,
                label_font: 12.0,
                trailing_font: 10.0,
                radio_outer_r: 4.5,
                radio_slot_w: 14.0,
                radio_pad_r: 5.0,
                radio_stroke_w: 1.2,
                radio_dot_inset: 1.8,
            },
            dropdown: DropdownTheme {
                row_h: crate::style::UNIT,
                item_h: 20.0,
                chevron_w: 14.0,
                pad_x: 8.0,
                text_font: 12.0,
                icon_size: 12.0,
                popup_gap: 2.0,
                popup_inner_margin: 2,
                item_spacing_y: 1.0,
                tint_rest: 0.06,
                tint_hover: 0.14,
                tint_press: 0.28,
            },
            slider: SliderTheme {
                row_h: 18.0,
                value_font: 11.0,
            },
            toggle: ToggleTheme {
                row_h: 18.0,
                track_w: 38.0,
                label_track_gap: 6.0,
                knob_pad: 2.0,
                track_accent_hint: 0.22,
            },
            readout: ReadoutTheme {
                row_h: crate::style::UNIT,
                label_font: 12.0,
                value_font: 11.0,
                edge_pad: 8.0,
            },
            color: ColorTheme {
                row_h: 20.0,
                swatch_w: 72.0,
                label_font: 12.0,
                label_pad_l: 4.0,
                picker_gap: 4.0,
            },
            chip: ChipTheme {
                height: 16.0,
                pad_x: 6.0,
            },
            keybinding: KeybindingTheme {
                row_h: crate::style::UNIT,
                key_font: 11.0,
                action_font: 11.0,
                key_pad_x: 5.0,
                key_pad_y: 1.0,
                key_action_gap: 8.0,
            },
            badge: BadgeTheme {
                row_h: crate::style::UNIT,
                label_col_w: 96.0,
                label_font: 11.0,
                label_pad_x: 8.0,
                label_chips_gap: 6.0,
                chip_gap_x: 3.0,
            },
            drag_value: DragValueTheme {
                row_h: 18.0,
                input_w: 72.0,
            },
        },
        icons: IconTheme {
            section_inline_scale: 1.2,
            section_icon_title_gap: 6.0,
            section_chevron_w: 14.0,
            section_chevron_gap: 2.0,
            overlay_icon_scale: 0.55,
            overlay_arrow_stroke_w: 1.4,
            overlay_arrow_shrink: 5.0,
            overlay_arrow_tip_t: 0.65,
            overlay_arrow_head_len: 4.0,
            overlay_arrow_head_half_w: 2.6,
            tree_type_icon_size: 12.0,
            tree_glyph_icon_size: 12.0,
        },
        pane: PaneTheme {
            inner_margin: 2.0,
            outer_span_default: 320.0,
            title_strip_thickness: 25.0,
            body_animation_time: 0.18,
            default_flow_open: 280.0,
            resize_handle_thickness: 10.0,
            min_user_flow: 80.0,
            max_user_flow: 1200.0,
            min_user_span: 120.0,
            max_user_span: 1200.0,
            rail_panel_gap: 8.0,
            fill_visible: false,
            shadow_blur: 0,
            shadow_y: 0,
            show_title_divider: false,
            title_stripes: true,
            title_chromatic_aberration: true,
            title_brackets: true,
        },
        ribbon: RibbonTheme {
            side_button_size: 34.0,
            side_button_gap: 4.0,
            edge_gap: 8.0,
            panel_gap: 4.0,
            button_accent_fill: true,
            ghost_fill_alpha: 90,
            ghost_stroke_width: 0.0,
            permanent: RibbonChromeTheme {
                button_size: 34.0,
                button_gap: 4.0,
                edge_gap: 8.0,
                panel_gap: 4.0,
                ghost_fill_alpha: 90,
                ghost_stroke_width: 0.0,
            },
            view_local: RibbonChromeTheme {
                button_size: 34.0,
                button_gap: 4.0,
                edge_gap: 8.0,
                panel_gap: 4.0,
                ghost_fill_alpha: 90,
                ghost_stroke_width: 0.0,
            },
            workspace: RibbonChromeTheme {
                button_size: 34.0,
                button_gap: 4.0,
                edge_gap: 8.0,
                panel_gap: 4.0,
                ghost_fill_alpha: 90,
                ghost_stroke_width: 0.0,
            },
            slot_override_transition: 0.08,
            active_view_indicator: ActiveIndicatorTheme {
                thickness: 2.0,
                inset: 4.0,
                alpha: 220,
            },
        },
        shelf: ShelfTheme {
            side_default_size: 320.0,
            bottom_default_size: 260.0,
            min_size: 160.0,
            max_size: 760.0,
            padding: 4.0,
            resize_handle_thickness: 10.0,
            background_alpha: 238,
            border_width: 1.0,
        },
        container: ContainerTheme {
            title_zone_thickness: 22.0,
            title_inset: 6.0,
            divider_inset: 6.0,
            title_body_gap_half: 4.0,
            default_width: 280.0,
            default_height: 280.0,
            default_min_width: 286.0,
            pod_pad_x: 8,
            pod_pad_y: 3,
            fill_mode: ColorMode::FromAccent {
                lerp_factor,
                lerp_target,
            },
            show_frame: true,
            show_title_divider: false,
            pad_x: 3,
            pad_y: 2,
            body_indent: 8.0,
            outer_margin_flow_title: 6,
            outer_margin_flow_body: 0,
            outer_margin_span: 1,
            body_inner_top_pad: 12.0,
            animation_time: 0.35,
            gap: 4.0,
            corner_ticks_inset: 3.0,
            title_brackets: true,
            title_prefix: None,
            title_letter_spacing: 1.5,
            bottom_rule: true,
            show_chevron: false,
            title_strip_filled: true,
            title_size: 11.5,
            icon_at_end: true,
            icon_size: 20.0,
            body_top_pad: 16.0,
            title_trailing_rule: false,
            corner_ticks: 10.0,
            separator_strip_h: 14.0,
            separator_alpha: 64,
            body_inner_end_pad: 12.0,
        },
        border_subtle: if dark {
            egui::Color32::from_rgb(0x80, 0x80, 0x80)
        } else {
            egui::Color32::from_rgb(0x6B, 0x70, 0x78)
        },
        border_inner: egui::Color32::from_rgb(0x1F, 0x26, 0x38),
        border_alpha: if dark { 35 } else { 0 },
        border_accent_tint: 0.0,
        border_width: if dark { 0.63 } else { 0.0 },
        row_separator_alpha: if dark { 25 } else { 28 },
        glass_card_factor: 1.0,
        glass_group_factor: 1.0,
        glass_accent_tint: 0.0,
        radius_widget: 0,
        radius_compact: 0,
        radius_sm: 0,
        radius_md: 0,
        radius_lg: 0,
        row_alternation: false,
        row_alt_lift: 0.0,
        button_full_accent_on_press: true,
        button_tint_rest: 0.12,
        button_tint_hover: 0.18,
        button_tint_press: 0.40,
        pane_shadow_blur: 0,
        pane_shadow_y: 0,
        pane_show_title_divider: false,
        pane_title_stripes: true,
        scramble_titles: true,
        tree_guide_width: 0.0,
        graph_pin_width: 0.0,
        graph_wire_glow: 1.0,
        graph_pin_glow: 0.85,
        graph_canvas_hex: true,
        progressbar_segmented: true,
        pane_title_brackets: true,
        section_separator_strip_h: 14.0,
        section_separator_alpha: 64,
        section_body_inner_end_pad: 12.0,
        ghost_fill_alpha: 90,
        ghost_stroke_width: 0.0,
        pastel_accent: true,
    }
}
