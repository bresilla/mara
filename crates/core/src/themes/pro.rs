//! PRO theme — soft glass, rounded corners, subtle accent-tinted
//! borders. Default theme on first launch.
//!
//! See [`crate::themes`](super) for the "how to add a theme" guide.

use crate::style::{
    // shared (dark-mode) palette + text constants
    AXIS_X,
    AXIS_Y,
    AXIS_Z,
    ActiveIndicatorTheme,
    BG_0_WINDOW,
    BG_1_PANEL,
    BG_2_RAISED,
    BG_3_HOVER,
    BG_4_INPUT,
    BORDER_INNER,
    BORDER_SUBTLE,
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
    radius,
};

/// PRO Light surface palette — paper-tinted neutrals matching
/// GitHub Primer's light-mode tokens. Text colours are NOT defined
/// here; they come from the shared `TEXT_*_LIGHT` constants so all
/// light variants pick the same body-text tones.
pub(crate) const PRO_LIGHT_BG_WINDOW: crate::vocab::Color32 =
    crate::vocab::Color32::from_rgb(0xF5, 0xF5, 0xF7);
pub(crate) const PRO_LIGHT_BG_PANEL: crate::vocab::Color32 =
    crate::vocab::Color32::from_rgb(0xFF, 0xFF, 0xFF);
// Raised + input tiers tightened — the previous values (`F6F8FA`
// raised, `FAFAFC` input) sat ~5 units off the white panel, so
// dropdowns and button surfaces were effectively invisible. Mirrors
// the Dark tier deltas (panel ± ~12 units) inverted toward darker
// grey.
pub(crate) const PRO_LIGHT_BG_RAISED: crate::vocab::Color32 =
    crate::vocab::Color32::from_rgb(0xF1, 0xF3, 0xF6);
pub(crate) const PRO_LIGHT_BG_HOVER: crate::vocab::Color32 =
    crate::vocab::Color32::from_rgb(0xE6, 0xE8, 0xEC);
pub(crate) const PRO_LIGHT_BG_INPUT: crate::vocab::Color32 =
    crate::vocab::Color32::from_rgb(0xEF, 0xF1, 0xF4);
pub(crate) const PRO_LIGHT_BORDER_SUBTLE: crate::vocab::Color32 =
    crate::vocab::Color32::from_rgb(0xD1, 0xD9, 0xE0);
pub(crate) const PRO_LIGHT_BORDER_INNER: crate::vocab::Color32 =
    crate::vocab::Color32::from_rgb(0xC5, 0xCC, 0xD3);

/// Built-in PRO profile — soft glass, rounded corners, subtle
/// accent-tinted borders. Pick a [`Mode`] to flip between the
/// original dark surfaces and a paper-tinted light variant; every
/// other field (shape / chrome / brackets) is shared across modes.
pub fn theme_pro(mode: Mode) -> Theme {
    let dark = matches!(mode, Mode::Dark);
    Theme {
        id: ThemeId {
            family: "PRO",
            variant: if dark { "DARK" } else { "LIGHT" },
        },
        name: if dark { "PRO_DARK" } else { "PRO_LIGHT" },
        is_light: !dark,
        palette: PaletteTheme {
            bg_window: (if dark {
                BG_0_WINDOW
            } else {
                PRO_LIGHT_BG_WINDOW
            })
            .into(),
            bg_panel: (if dark { BG_1_PANEL } else { PRO_LIGHT_BG_PANEL }).into(),
            bg_raised: (if dark {
                BG_2_RAISED
            } else {
                PRO_LIGHT_BG_RAISED
            })
            .into(),
            bg_hover: (if dark { BG_3_HOVER } else { PRO_LIGHT_BG_HOVER }).into(),
            bg_input: (if dark { BG_4_INPUT } else { PRO_LIGHT_BG_INPUT }).into(),
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
            border_subtle: (if dark {
                BORDER_SUBTLE
            } else {
                PRO_LIGHT_BORDER_SUBTLE
            })
            .into(),
            border_inner: (if dark {
                BORDER_INNER
            } else {
                PRO_LIGHT_BORDER_INNER
            })
            .into(),
        },
        stroke: StrokeTheme {
            border_alpha: if dark { 70 } else { 100 },
            border_accent_tint: 0.06,
            border_width: 1.0,
            row_separator_alpha: if dark { 35 } else { 50 },
            row_separator_dash: None,
        },
        glass: GlassTheme {
            card_factor: 0.92,
            group_factor: 0.78,
            accent_tint: 0.03,
        },
        shape: ShapeTheme {
            radius_widget: radius::WIDGET,
            radius_compact: radius::COMPACT,
            radius_sm: radius::SM,
            radius_md: radius::MD,
            radius_lg: radius::LG,
        },
        text: TextTheme {
            title_color_mode: TextColorMode::Accent,
            title_softness: 0.0,
            body_accent_darken: 0.0,
        },
        motion: MotionTheme {
            animations_enabled: true,
            button_anim_scale: 1.0,
            pane_fade_scale: 0.5,
            scramble_titles: false,
        },
        graph: GraphTheme {
            node_pad_x: 8,
            node_pad_y: 4,
            bg_inner_margin: 2,
            canvas_pattern: GraphCanvasPattern::Dots {
                spacing: 30.0,
                radius: 1.0,
            },
            grid_alpha: 28,
            pin_stroke_width: 1.0,
            pin_stroke_alpha: 160,
            wire_width: 2.0,
            wire_glow: 0.6,
            pin_glow: 0.5,
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
            switcher_layout: ViewSwitcherLayout::Horizontal,
            switcher_button_min: 34.0,
            active_indicator_thickness: 2.0,
            active_indicator_inset: 4.0,
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
        bg_window: (if dark {
            BG_0_WINDOW
        } else {
            PRO_LIGHT_BG_WINDOW
        })
        .into(),
        bg_panel: (if dark { BG_1_PANEL } else { PRO_LIGHT_BG_PANEL }).into(),
        bg_raised: (if dark {
            BG_2_RAISED
        } else {
            PRO_LIGHT_BG_RAISED
        })
        .into(),
        bg_hover: (if dark { BG_3_HOVER } else { PRO_LIGHT_BG_HOVER }).into(),
        bg_input: (if dark { BG_4_INPUT } else { PRO_LIGHT_BG_INPUT }).into(),
        panel_fill_mode: ColorMode::FromBg,
        section_fill_mode: ColorMode::FromBg,
        section_show_frame: true,
        section_show_title_divider: true,
        section_pad_x: 2,
        section_pad_y: 2,
        section_body_indent: 8.0,
        section_outer_margin_flow_title: 3,
        section_outer_margin_flow_body: 3,
        section_outer_margin_span: 3,
        section_body_inner_top_pad: 0.0,
        pane_title_chromatic_aberration: false,
        // PRO — quick snappy fold / unfold so flipping sections
        // open while inspecting feels responsive.
        section_animation_time: 0.06,
        animations_enabled: true,
        button_anim_scale: 1.0,
        pane_fade_scale: 0.5,
        // Text — pulled from the SHARED light/dark tone constants so
        // every variant ends up with the same body-text colours.
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
        ribbon_button_accent_fill: false,
        section_gap: 0.0,
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
        tabs: TabTheme {
            layout: TabLayout::FolderSideStrip,
            outer_inset: TabOuterInset::MirrorBodyInset,
            strip_thickness: 32.0,
            tab_len: 36.0,
            tab_gap: 6.0,
            tab_overlap: 2.5,
            title_row_height_multiplier: 1.0,
            folder_icon_size: 20.0,
            folder_active_radius: 7,
            inactive_glyph_color: TabInactiveGlyphColor::TextSecondary,
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
                full_accent_on_press: false,
                tint_rest: 0.08,
                tint_hover: 0.16,
                tint_press: 0.30,
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
            ghost_fill_alpha: 28,
            ghost_stroke_width: 1.5,
            permanent: RibbonChromeTheme {
                button_size: 34.0,
                button_gap: 4.0,
                edge_gap: 8.0,
                panel_gap: 4.0,
                ghost_fill_alpha: 28,
                ghost_stroke_width: 1.5,
            },
            view_local: RibbonChromeTheme {
                button_size: 34.0,
                button_gap: 4.0,
                edge_gap: 8.0,
                panel_gap: 4.0,
                ghost_fill_alpha: 28,
                ghost_stroke_width: 1.5,
            },
            workspace: RibbonChromeTheme {
                button_size: 34.0,
                button_gap: 4.0,
                edge_gap: 8.0,
                panel_gap: 4.0,
                ghost_fill_alpha: 28,
                ghost_stroke_width: 1.5,
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
            min_size: 160.0,
            max_size: 720.0,
            padding: 6.0,
            resize_handle_thickness: 8.0,
            background_alpha: 224,
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
            fill_mode: ColorMode::FromBg,
            show_frame: true,
            show_title_divider: true,
            pad_x: 2,
            pad_y: 2,
            body_indent: 8.0,
            outer_margin_flow_title: 3,
            outer_margin_flow_body: 3,
            outer_margin_span: 3,
            body_inner_top_pad: 0.0,
            animation_time: 0.06,
            gap: 0.0,
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
            separator_alpha: 128,
            body_inner_end_pad: 0.0,
        },
        border_subtle: (if dark {
            BORDER_SUBTLE
        } else {
            PRO_LIGHT_BORDER_SUBTLE
        })
        .into(),
        border_inner: (if dark {
            BORDER_INNER
        } else {
            PRO_LIGHT_BORDER_INNER
        })
        .into(),
        border_alpha: if dark { 70 } else { 100 },
        border_accent_tint: 0.06,
        border_width: 1.0,
        row_separator_alpha: if dark { 35 } else { 50 },
        glass_card_factor: 0.92,
        glass_group_factor: 0.78,
        glass_accent_tint: 0.03,
        radius_widget: radius::WIDGET,
        radius_compact: radius::COMPACT,
        radius_sm: radius::SM,
        radius_md: radius::MD,
        radius_lg: radius::LG,
        row_alternation: false,
        row_alt_lift: 0.0,
        button_full_accent_on_press: false,
        button_tint_rest: 0.08,
        button_tint_hover: 0.16,
        button_tint_press: 0.30,
        pane_shadow_blur: 24,
        pane_shadow_y: 8,
        pane_show_title_divider: true,
        pane_title_stripes: false,
        scramble_titles: false,
        tree_guide_width: 1.0,
        graph_pin_width: 1.0,
        graph_wire_glow: 0.6,
        graph_pin_glow: 0.5,
        graph_canvas_hex: false,
        progressbar_segmented: false,
        pane_title_brackets: false,
        section_separator_strip_h: 2.0,
        section_separator_alpha: 128,
        section_body_inner_end_pad: 0.0,
        ghost_fill_alpha: 28,
        ghost_stroke_width: 1.5,
        pastel_accent: true,
    }
}
