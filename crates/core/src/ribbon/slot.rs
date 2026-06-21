use std::collections::HashSet;

use crate::icons;
use crate::view::ViewId;
use crate::vocab::Id;

use super::{RibbonAction, RibbonCluster, RibbonEdge, RibbonMode, RibbonRole};

/// Scope that decides when a slot-based ribbon participates in
/// resolution.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RibbonScope {
    Permanent,
    View(ViewId),
    WorkspaceLevel(Id),
}

/// Stable id for an overridable ribbon slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct RibbonSlotId(pub Id);

impl RibbonSlotId {
    #[must_use]
    pub fn new(source: impl std::hash::Hash) -> Self {
        Self(Id::new(source))
    }
}

/// Whether an active view/workspace layer may alter a permanent slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RibbonOverridePolicy {
    Fixed,
    LayerOverride,
    LayerAppend,
}

/// Slot-aware ribbon item. This is the single public ribbon button
/// declaration; featureful chrome fields let it keep drag, panel,
/// and fullscreen behavior without exposing a second button type.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RibbonSlotItem {
    pub id: Id,
    /// Optional stable id for featureful chrome.
    ///
    /// When this is present, the slot item can participate in the
    /// same chrome path as drag/reorderable ribbons, panel toggles,
    /// pane anchoring, and fullscreen rails. This is how the slot API
    /// and the original featureful API converge instead of competing.
    pub chrome_id: Option<&'static str>,
    pub chrome_tooltip: Option<&'static str>,
    pub icon: &'static str,
    pub label: String,
    pub tooltip: String,
    pub action: RibbonAction,
    pub active: bool,
    pub draggable: bool,
    pub role: Option<RibbonRole>,
    pub child_ribbon: Option<&'static str>,
}

impl RibbonSlotItem {
    #[must_use]
    pub fn new(
        id: impl Into<Id>,
        icon: &'static str,
        label: impl Into<String>,
        tooltip: impl Into<String>,
        action: RibbonAction,
    ) -> Self {
        let label = label.into();
        let tooltip = tooltip.into();
        assert_ribbon_icon(icon);
        assert!(
            !label.trim().is_empty(),
            "ribbon slot items require a non-empty label"
        );
        assert!(
            !tooltip.trim().is_empty(),
            "ribbon slot items require a non-empty tooltip"
        );
        Self {
            id: id.into(),
            chrome_id: None,
            chrome_tooltip: None,
            icon,
            label,
            tooltip,
            action,
            active: false,
            draggable: false,
            role: None,
            child_ribbon: None,
        }
    }

    /// Construct a slot item that is immediately eligible for the
    /// featureful chrome.
    ///
    /// Use this for app chrome that needs the original ribbon
    /// capabilities: draggable placement, panel toggles, live pane
    /// anchors, and fullscreen layering. The same item still carries
    /// slot actions/override semantics.
    #[must_use]
    pub fn featureful(
        id: &'static str,
        icon: &'static str,
        label: impl Into<String>,
        tooltip: &'static str,
        action: RibbonAction,
    ) -> Self {
        Self::new(Id::new(id), icon, label, tooltip, action)
            .with_chrome_id(id)
            .with_chrome_tooltip(tooltip)
    }

    #[must_use]
    pub fn with_chrome_id(mut self, id: &'static str) -> Self {
        assert!(
            !id.trim().is_empty(),
            "featureful ribbon items require a non-empty chrome id"
        );
        self.chrome_id = Some(id);
        self
    }

    #[must_use]
    pub fn with_chrome_tooltip(mut self, tooltip: &'static str) -> Self {
        assert!(
            !tooltip.trim().is_empty(),
            "featureful ribbon items require a non-empty chrome tooltip"
        );
        self.chrome_tooltip = Some(tooltip);
        self
    }

    #[must_use]
    pub fn with_role(mut self, role: RibbonRole) -> Self {
        self.role = Some(role);
        self
    }

    #[must_use]
    pub fn draggable(mut self, draggable: bool) -> Self {
        self.draggable = draggable;
        self
    }

    #[must_use]
    pub fn as_panel_button(mut self) -> Self {
        self.role = Some(RibbonRole::Panel);
        self
    }

    #[must_use]
    pub fn as_icon_button(mut self) -> Self {
        self.role = Some(RibbonRole::Icon);
        self
    }

    #[must_use]
    pub fn with_child_ribbon(mut self, child: &'static str) -> Self {
        assert!(
            !child.trim().is_empty(),
            "featureful ribbon items require a non-empty child ribbon id"
        );
        self.child_ribbon = Some(child);
        self
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RibbonSlot {
    pub id: RibbonSlotId,
    pub default_item: Option<RibbonSlotItem>,
    pub override_policy: RibbonOverridePolicy,
}

impl RibbonSlot {
    #[must_use]
    pub fn new(
        id: RibbonSlotId,
        default_item: Option<RibbonSlotItem>,
        override_policy: RibbonOverridePolicy,
    ) -> Self {
        if let Some(item) = &default_item {
            validate_ribbon_slot_item(item);
        }
        Self {
            id,
            default_item,
            override_policy,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RibbonSlotOverride {
    pub slot: RibbonSlotId,
    pub item: Option<RibbonSlotItem>,
}

impl RibbonSlotOverride {
    #[must_use]
    pub fn new(slot: RibbonSlotId, item: RibbonSlotItem) -> Self {
        validate_ribbon_slot_item(&item);
        Self {
            slot,
            item: Some(item),
        }
    }

    /// Explicitly hide a slot for the active layer.
    ///
    /// This is the API-level opt-out for persistent bar icons: the
    /// main bar and its slots stay registered, but a view/workspace
    /// can intentionally suppress one inherited icon.
    #[must_use]
    pub fn hidden(slot: RibbonSlotId) -> Self {
        Self { slot, item: None }
    }

    #[must_use]
    pub fn is_hidden(&self) -> bool {
        self.item.is_none()
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RibbonOverrideLayer {
    pub overrides: Vec<RibbonSlotOverride>,
}

impl RibbonOverrideLayer {
    #[must_use]
    pub fn new(overrides: Vec<RibbonSlotOverride>) -> Self {
        validate_ribbon_override_layer_parts(&overrides);
        Self { overrides }
    }

    #[must_use]
    pub fn find(&self, slot: RibbonSlotId) -> Option<&RibbonSlotOverride> {
        self.overrides
            .iter()
            .find(|candidate| candidate.slot == slot)
    }

    #[must_use]
    pub fn with_hidden_slot(mut self, slot: RibbonSlotId) -> Self {
        assert!(
            self.overrides
                .iter()
                .all(|candidate| candidate.slot != slot),
            "ribbon override layers require unique slot ids"
        );
        self.overrides.push(RibbonSlotOverride::hidden(slot));
        self
    }
}

/// Slot-based ribbon declaration. This is the single public ribbon
/// declaration; featureful chrome fields let it keep drag, panel,
/// and fullscreen behavior without exposing a second ribbon type.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RibbonSlotDef {
    pub id: Id,
    /// Optional stable id for featureful chrome.
    pub chrome_id: Option<&'static str>,
    pub scope: RibbonScope,
    pub edge: RibbonEdge,
    pub role: RibbonRole,
    pub mode: RibbonMode,
    pub cluster: RibbonCluster,
    pub accepts: &'static [&'static str],
    pub slots: Vec<RibbonSlot>,
}

impl RibbonSlotDef {
    #[must_use]
    pub fn new(
        id: impl Into<Id>,
        scope: RibbonScope,
        edge: RibbonEdge,
        cluster: RibbonCluster,
        slots: Vec<RibbonSlot>,
    ) -> Self {
        validate_ribbon_slots(&slots);
        Self {
            id: id.into(),
            chrome_id: None,
            scope,
            edge,
            role: RibbonRole::Panel,
            mode: RibbonMode::ThreeSided,
            cluster,
            accepts: &[],
            slots,
        }
    }

    /// Construct a slot ribbon that is immediately eligible for the
    /// featureful chrome while still participating in scope/layer
    /// resolution.
    #[must_use]
    pub fn featureful(
        id: &'static str,
        scope: RibbonScope,
        edge: RibbonEdge,
        cluster: RibbonCluster,
        slots: Vec<RibbonSlot>,
    ) -> Self {
        Self::new(Id::new(id), scope, edge, cluster, slots).with_chrome_id(id)
    }

    #[must_use]
    pub fn with_chrome_id(mut self, id: &'static str) -> Self {
        assert!(
            !id.trim().is_empty(),
            "featureful ribbon definitions require a non-empty chrome id"
        );
        self.chrome_id = Some(id);
        self
    }

    #[must_use]
    pub fn with_role(mut self, role: RibbonRole) -> Self {
        self.role = role;
        self
    }

    #[must_use]
    pub fn with_mode(mut self, mode: RibbonMode) -> Self {
        self.mode = mode;
        self
    }

    #[must_use]
    pub fn accepts(mut self, accepts: &'static [&'static str]) -> Self {
        assert!(
            accepts.iter().all(|accept| !accept.trim().is_empty()),
            "featureful ribbon definitions require non-empty accepted payload tags"
        );
        self.accepts = accepts;
        self
    }
}

fn assert_unique_slot_ids(slots: &[RibbonSlot]) {
    let mut seen = HashSet::with_capacity(slots.len());
    assert!(
        slots.iter().all(|slot| seen.insert(slot.id)),
        "ribbon definitions require unique slot ids"
    );
}

pub(crate) fn validate_ribbon_slot_item(item: &RibbonSlotItem) {
    assert_ribbon_icon(item.icon);
    assert!(
        !item.label.trim().is_empty(),
        "ribbon slot items require a non-empty label"
    );
    assert!(
        !item.tooltip.trim().is_empty(),
        "ribbon slot items require a non-empty tooltip"
    );
    if let Some(chrome_id) = item.chrome_id {
        assert!(
            !chrome_id.trim().is_empty(),
            "featureful ribbon items require a non-empty chrome id"
        );
    }
    if let Some(chrome_tooltip) = item.chrome_tooltip {
        assert!(
            !chrome_tooltip.trim().is_empty(),
            "featureful ribbon items require a non-empty chrome tooltip"
        );
    }
    if let Some(child) = item.child_ribbon {
        assert!(
            !child.trim().is_empty(),
            "featureful ribbon items require a non-empty child ribbon id"
        );
    }
}

fn assert_ribbon_icon(icon: &'static str) {
    assert!(
        !icon.trim().is_empty(),
        "ribbon slot items require a non-empty icon"
    );
    assert!(
        icons::is_icon_payload(icon),
        "ribbon slot items require an icon that resolves to a bundled font icon or inline SVG"
    );
}

pub(crate) fn validate_ribbon_slot(slot: &RibbonSlot) {
    if let Some(item) = &slot.default_item {
        validate_ribbon_slot_item(item);
    }
}

pub(crate) fn validate_ribbon_slots(slots: &[RibbonSlot]) {
    assert_unique_slot_ids(slots);
    slots.iter().for_each(validate_ribbon_slot);
}

pub(crate) fn validate_ribbon_override_layer(layer: &RibbonOverrideLayer) {
    validate_ribbon_override_layer_parts(&layer.overrides);
}

pub(crate) fn validate_ribbon_slot_def(ribbon: &RibbonSlotDef) {
    if let Some(chrome_id) = ribbon.chrome_id {
        assert!(
            !chrome_id.trim().is_empty(),
            "featureful ribbon definitions require a non-empty chrome id"
        );
    }
    assert!(
        ribbon
            .accepts
            .iter()
            .all(|accept| !accept.trim().is_empty()),
        "featureful ribbon definitions require non-empty accepted payload tags"
    );
    validate_ribbon_slots(&ribbon.slots);
}

fn validate_ribbon_override_layer_parts(overrides: &[RibbonSlotOverride]) {
    let mut seen = HashSet::with_capacity(overrides.len());
    assert!(
        overrides
            .iter()
            .all(|slot_override| seen.insert(slot_override.slot)),
        "ribbon override layers require unique slot ids"
    );
    overrides
        .iter()
        .filter_map(|slot_override| slot_override.item.as_ref())
        .for_each(validate_ribbon_slot_item);
}
