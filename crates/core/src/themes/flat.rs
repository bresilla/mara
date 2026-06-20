//! FLAT theme — high-contrast, no-glass proof theme.
//!
//! This intentionally reuses the generic renderers and typed theme
//! contracts. It exists to prove a third visual family can be added
//! as data rather than with renderer-side `if theme == ...` branches.

use crate::style::{
    ActiveIndicatorTheme, BadgeTheme, ButtonTheme, ChipTheme, CodeTheme, ColorMode, ColorTheme,
    DragValueTheme, DropdownTheme, GlassTheme, GraphCanvasPattern, GraphTheme, IconTheme,
    KeybindingTheme, Mode, ModuleTheme, MotionTheme, OverlayTheme, PaletteTheme, PaneTheme,
    PodTheme, ProgressTheme, ReadoutTheme, RibbonChromeTheme, RibbonTheme, SelectTheme, ShapeTheme,
    ShelfTheme, SliderTheme, StrokeTheme, TabInactiveGlyphColor, TabLayout, TabOuterInset,
    TabTheme, TextColorMode, TextTheme, Theme, ThemeId, ToggleTheme, TreeTheme, ViewSwitcherLayout,
    ViewTheme, WidgetTheme, WindowChromeTheme,
};

pub(crate) const FLAT_DARK_BG_WINDOW: egui::Color32 = egui::Color32::from_rgb(0x00, 0x00, 0x00);
pub(crate) const FLAT_DARK_BG_PANEL: egui::Color32 = egui::Color32::from_rgb(0x10, 0x10, 0x10);
pub(crate) const FLAT_DARK_BG_RAISED: egui::Color32 = egui::Color32::from_rgb(0x1C, 0x1C, 0x1C);
pub(crate) const FLAT_DARK_BG_HOVER: egui::Color32 = egui::Color32::from_rgb(0x28, 0x28, 0x28);
pub(crate) const FLAT_DARK_BG_INPUT: egui::Color32 = egui::Color32::from_rgb(0x08, 0x08, 0x08);

pub(crate) const FLAT_LIGHT_BG_WINDOW: egui::Color32 = egui::Color32::from_rgb(0xF8, 0xF8, 0xF8);
pub(crate) const FLAT_LIGHT_BG_PANEL: egui::Color32 = egui::Color32::from_rgb(0xFF, 0xFF, 0xFF);
pub(crate) const FLAT_LIGHT_BG_RAISED: egui::Color32 = egui::Color32::from_rgb(0xEB, 0xEB, 0xEB);
pub(crate) const FLAT_LIGHT_BG_HOVER: egui::Color32 = egui::Color32::from_rgb(0xDD, 0xDD, 0xDD);
pub(crate) const FLAT_LIGHT_BG_INPUT: egui::Color32 = egui::Color32::from_rgb(0xF2, 0xF2, 0xF2);

pub const fn theme_flat(mode: Mode) -> Theme {
    let dark = matches!(mode, Mode::Dark);
    let bg_window = if dark {
        FLAT_DARK_BG_WINDOW
    } else {
        FLAT_LIGHT_BG_WINDOW
    };
    let bg_panel = if dark {
        FLAT_DARK_BG_PANEL
    } else {
        FLAT_LIGHT_BG_PANEL
    };
    let bg_raised = if dark {
        FLAT_DARK_BG_RAISED
    } else {
        FLAT_LIGHT_BG_RAISED
    };
    let bg_hover = if dark {
        FLAT_DARK_BG_HOVER
    } else {
        FLAT_LIGHT_BG_HOVER
    };
    let bg_input = if dark {
        FLAT_DARK_BG_INPUT
    } else {
        FLAT_LIGHT_BG_INPUT
    };
    let text_primary = if dark {
        egui::Color32::WHITE
    } else {
        egui::Color32::BLACK
    };
    let text_secondary = if dark {
        egui::Color32::from_rgb(0xC8, 0xC8, 0xC8)
    } else {
        egui::Color32::from_rgb(0x30, 0x30, 0x30)
    };
    let text_disabled = if dark {
        egui::Color32::from_rgb(0x70, 0x70, 0x70)
    } else {
        egui::Color32::from_rgb(0x90, 0x90, 0x90)
    };
    let border = if dark {
        egui::Color32::from_rgb(0xE0, 0xE0, 0xE0)
    } else {
        egui::Color32::from_rgb(0x20, 0x20, 0x20)
    };

    Theme {
        id: ThemeId {
            family: "FLAT",
            variant: if dark { "DARK" } else { "LIGHT" },
        },
        name: if dark { "FLAT_DARK" } else { "FLAT_LIGHT" },
        is_light: !dark,
        palette: PaletteTheme {
            bg_window,
            bg_panel,
            bg_raised,
            bg_hover,
            bg_input,
            text_primary,
            text_secondary,
            text_disabled,
            border_subtle: border,
            border_inner: border,
        },
        stroke: StrokeTheme {
            border_alpha: 220,
            border_accent_tint: 0.0,
            border_width: 2.0,
            row_separator_alpha: 120,
            row_separator_dash: None,
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
            title_color_mode: TextColorMode::Primary,
            title_softness: 0.0,
            body_accent_darken: 0.0,
        },
        motion: MotionTheme {
            animations_enabled: false,
            button_anim_scale: 0.0,
            pane_fade_scale: 0.0,
            scramble_titles: false,
        },
        graph: GraphTheme {
            node_pad_x: 8,
            node_pad_y: 4,
            bg_inner_margin: 2,
            canvas_pattern: GraphCanvasPattern::Dots {
                spacing: 24.0,
                radius: 1.0,
            },
            grid_alpha: 64,
            pin_stroke_width: 2.0,
            pin_stroke_alpha: 220,
            wire_width: 2.0,
            wire_glow: 0.0,
            pin_glow: 0.0,
            node_halo_gap: 2.0,
            node_halo_width: 2.0,
            node_halo_radius_outset: 2,
        },
        code: CodeTheme {
            font_size: 13.0,
            line_height_factor: 1.2,
            min_rows: 6,
            force_dark: false,
            functions: if dark {
                egui::Color32::from_rgb(0x7F, 0xFF, 0x7F)
            } else {
                egui::Color32::from_rgb(0x00, 0x70, 0x00)
            },
            literals: if dark {
                egui::Color32::from_rgb(0xFF, 0x7F, 0x7F)
            } else {
                egui::Color32::from_rgb(0xA0, 0x00, 0x00)
            },
            numerics: if dark {
                egui::Color32::from_rgb(0xFF, 0x7F, 0x7F)
            } else {
                egui::Color32::from_rgb(0xA0, 0x00, 0x00)
            },
            strings: if dark {
                egui::Color32::from_rgb(0xFF, 0xFF, 0x7F)
            } else {
                egui::Color32::from_rgb(0x80, 0x70, 0x00)
            },
            types: if dark {
                egui::Color32::from_rgb(0x7F, 0xD0, 0xFF)
            } else {
                egui::Color32::from_rgb(0x00, 0x50, 0xA0)
            },
        },
        overlay: OverlayTheme {
            inline_chip_size: 24.0,
            inline_chip_pad: 4.0,
            fullscreen_button_size: 34.0,
            fullscreen_edge_gap: 8.0,
            placeholder_text: "(maximised)",
            ghost_fill_alpha: 80,
            ghost_stroke_width: 2.0,
        },
        views: ViewTheme {
            switcher_layout: ViewSwitcherLayout::Horizontal,
            switcher_button_min: 34.0,
            active_indicator_thickness: 2.0,
            active_indicator_inset: 2.0,
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
        tabs: TabTheme {
            layout: TabLayout::FolderSideStrip,
            outer_inset: TabOuterInset::None,
            strip_thickness: 22.0,
            tab_len: 32.0,
            tab_gap: 2.0,
            tab_overlap: 0.0,
            title_row_height_multiplier: 1.0,
            folder_icon_size: 16.0,
            folder_active_radius: 0,
            inactive_glyph_color: TabInactiveGlyphColor::HighContrast,
        },
        pod: PodTheme {
            widget_spacing: 3.0,
            min_widget_h: crate::style::UNIT,
            max_widget_h: 240.0,
            tag_row_pitch: 19.0,
        },
        widgets: WidgetTheme {
            button: ButtonTheme {
                row_h: 22.0,
                subtitle_row_h: crate::widget::button::BUTTON_ROW_H_SUBTITLE,
                label_font: crate::widget::button::BUTTON_LABEL_FONT,
                subtitle_font: crate::widget::button::BUTTON_SUBTITLE_FONT,
                glyph_font: crate::widget::button::BUTTON_GLYPH_FONT,
                edge_pad: 6.0,
                glyph_w: 14.0,
                glyph_gap: 6.0,
                full_accent_on_press: false,
                tint_rest: 0.0,
                tint_hover: 0.0,
                tint_press: 0.0,
            },
            progress: ProgressTheme {
                row_h: crate::widget::progressbar::PROGRESSBAR_ROW_H,
                value_font: crate::widget::progressbar::PROGRESSBAR_VALUE_FONT,
                segmented: false,
                segments: 12,
                segment_gap: 1.5,
                segment_inset: 1.5,
                dim_alpha: 38,
            },
            tree: TreeTheme {
                row_h: crate::widget::tree::TREE_ROW_H,
                indent: crate::widget::tree::TREE_INDENT,
                guide_width: 1.0,
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
                label_pad_l: 8.0,
                trailing_pad_r: 5.0,
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
                pad_x: 6.0,
                text_font: 12.0,
                icon_size: 12.0,
                popup_gap: 2.0,
                popup_inner_margin: 2,
                item_spacing_y: 1.0,
                tint_rest: 0.0,
                tint_hover: 0.08,
                tint_press: 0.16,
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
                track_accent_hint: 0.18,
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
            section_inline_scale: 1.1,
            section_icon_title_gap: 5.0,
            section_chevron_w: 12.0,
            section_chevron_gap: 2.0,
            overlay_icon_scale: 0.50,
            overlay_arrow_stroke_w: 1.6,
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
            body_animation_time: 0.0,
            default_flow_open: 280.0,
            resize_handle_thickness: 10.0,
            min_user_flow: 80.0,
            max_user_flow: 1200.0,
            min_user_span: 120.0,
            max_user_span: 1200.0,
            rail_panel_gap: 8.0,
            fill_visible: true,
            shadow_blur: 0,
            shadow_y: 0,
            show_title_divider: true,
            title_stripes: false,
            title_chromatic_aberration: false,
            title_brackets: false,
        },
        ribbon: RibbonTheme {
            side_button_size: 34.0,
            side_button_gap: 4.0,
            edge_gap: 8.0,
            panel_gap: 4.0,
            button_accent_fill: false,
            ghost_fill_alpha: 100,
            ghost_stroke_width: 2.0,
            permanent: RibbonChromeTheme {
                button_size: 34.0,
                button_gap: 4.0,
                edge_gap: 8.0,
                panel_gap: 4.0,
                ghost_fill_alpha: 100,
                ghost_stroke_width: 2.0,
            },
            view_local: RibbonChromeTheme {
                button_size: 34.0,
                button_gap: 4.0,
                edge_gap: 8.0,
                panel_gap: 4.0,
                ghost_fill_alpha: 100,
                ghost_stroke_width: 2.0,
            },
            workspace: RibbonChromeTheme {
                button_size: 34.0,
                button_gap: 4.0,
                edge_gap: 8.0,
                panel_gap: 4.0,
                ghost_fill_alpha: 100,
                ghost_stroke_width: 2.0,
            },
            slot_override_transition: 0.08,
            active_view_indicator: ActiveIndicatorTheme {
                thickness: 2.0,
                inset: 4.0,
                alpha: 220,
            },
        },
        shelf: ShelfTheme {
            side_default_size: 300.0,
            bottom_default_size: 240.0,
            min_size: 150.0,
            max_size: 720.0,
            padding: 4.0,
            resize_handle_thickness: 8.0,
            background_alpha: 255,
            border_width: 1.0,
        },
        container: crate::style::ContainerTheme {
            title_zone_thickness: 22.0,
            title_inset: 5.0,
            divider_inset: 5.0,
            title_body_gap_half: 3.0,
            default_width: 280.0,
            default_height: 280.0,
            default_min_width: 286.0,
            pod_pad_x: 6,
            pod_pad_y: 2,
            fill_mode: ColorMode::FromBg,
            show_frame: true,
            show_title_divider: true,
            pad_x: 2,
            pad_y: 2,
            body_indent: 8.0,
            outer_margin_flow_title: 2,
            outer_margin_flow_body: 2,
            outer_margin_span: 2,
            body_inner_top_pad: 0.0,
            animation_time: 0.0,
            gap: 2.0,
            corner_ticks_inset: 0.0,
            title_brackets: false,
            title_prefix: None,
            title_letter_spacing: 0.0,
            bottom_rule: false,
            show_chevron: true,
            title_strip_filled: false,
            title_size: 11.0,
            icon_at_end: false,
            icon_size: 0.0,
            body_top_pad: 0.0,
            title_trailing_rule: false,
            corner_ticks: 0.0,
            separator_strip_h: 2.0,
            separator_alpha: 160,
            body_inner_end_pad: 0.0,
        },
        bg_window,
        bg_panel,
        bg_raised,
        bg_hover,
        bg_input,
        panel_fill_mode: ColorMode::FromBg,
        section_fill_mode: ColorMode::FromBg,
        section_show_frame: true,
        section_show_title_divider: true,
        section_pad_x: 2,
        section_pad_y: 2,
        section_body_indent: 8.0,
        section_outer_margin_flow_title: 2,
        section_outer_margin_flow_body: 2,
        section_outer_margin_span: 2,
        section_body_inner_top_pad: 0.0,
        pane_title_chromatic_aberration: false,
        section_animation_time: 0.0,
        animations_enabled: false,
        button_anim_scale: 0.0,
        pane_fade_scale: 0.0,
        text_primary,
        text_secondary,
        text_disabled,
        title_color_mode: TextColorMode::Primary,
        title_softness: 0.0,
        ribbon_button_accent_fill: false,
        section_gap: 2.0,
        section_corner_ticks_inset: 0.0,
        section_title_brackets: false,
        section_title_prefix: None,
        section_title_letter_spacing: 0.0,
        section_bottom_rule: false,
        pane_fill_visible: true,
        show_section_chevron: true,
        title_strip_filled: false,
        section_title_size: 11.0,
        body_accent_darken: 0.0,
        section_icon_at_end: false,
        section_icon_size: 0.0,
        section_body_top_pad: 0.0,
        row_separator_dash: None,
        section_title_trailing_rule: false,
        section_corner_ticks: 0.0,
        border_subtle: border,
        border_inner: border,
        border_alpha: 220,
        border_accent_tint: 0.0,
        border_width: 2.0,
        row_separator_alpha: 120,
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
        button_full_accent_on_press: false,
        button_tint_rest: 0.0,
        button_tint_hover: 0.0,
        button_tint_press: 0.0,
        pane_shadow_blur: 0,
        pane_shadow_y: 0,
        pane_show_title_divider: true,
        pane_title_stripes: false,
        scramble_titles: false,
        tree_guide_width: 1.0,
        graph_pin_width: 2.0,
        graph_wire_glow: 0.0,
        graph_pin_glow: 0.0,
        graph_canvas_hex: false,
        progressbar_segmented: false,
        pane_title_brackets: false,
        section_separator_strip_h: 2.0,
        section_separator_alpha: 160,
        section_body_inner_end_pad: 0.0,
        ghost_fill_alpha: 100,
        ghost_stroke_width: 2.0,
        pastel_accent: false,
    }
}
