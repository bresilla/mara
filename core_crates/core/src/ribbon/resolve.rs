use super::{
    RibbonOverrideLayer, RibbonOverridePolicy, RibbonSlot, RibbonSlotItem, RibbonSlotOverride,
    slot::{validate_ribbon_override_layer, validate_ribbon_slot},
};

/// Resolve a slot to the items that should paint in that slot.
///
/// `layers` must be passed from shallowest to deepest:
/// permanent/default context, active view, L1, L2, ...
/// The resolver scans from the deepest layer back toward the view,
/// so L2 beats L1, L1 beats view, and default wins only when no
/// allowed override exists.
#[must_use]
pub fn resolve_slot_items(
    slot: &RibbonSlot,
    layers: &[RibbonOverrideLayer],
) -> Vec<RibbonSlotItem> {
    validate_ribbon_slot(slot);
    layers.iter().for_each(validate_ribbon_override_layer);
    match slot.override_policy {
        RibbonOverridePolicy::Fixed => slot.default_item.iter().cloned().collect(),
        RibbonOverridePolicy::LayerOverride => {
            for layer in layers.iter().rev() {
                if let Some(override_slot) = layer.find(slot.id) {
                    return override_slot.item.iter().cloned().collect();
                }
            }
            slot.default_item.iter().cloned().collect()
        }
        RibbonOverridePolicy::LayerAppend => {
            let mut items: Vec<RibbonSlotItem> = slot.default_item.iter().cloned().collect();
            for item in layers
                .iter()
                .flat_map(|layer| layer.overrides.iter())
                .filter_map(|candidate: &RibbonSlotOverride| {
                    (candidate.slot == slot.id)
                        .then(|| candidate.item.clone())
                        .flatten()
                })
            {
                items.push(item);
            }
            items
        }
    }
}

/// Convenience for the common replacement case.
#[must_use]
pub fn resolve_slot_item(
    slot: &RibbonSlot,
    layers: &[RibbonOverrideLayer],
) -> Option<RibbonSlotItem> {
    resolve_slot_items(slot, layers).into_iter().next()
}
