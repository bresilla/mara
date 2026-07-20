#!/usr/bin/env bash
# Coupling ratchet — PLAN.md Phase 0.2 / ADR 0001.
#
# Counts four egui-coupling metrics in mara_core and fails if any count
# EXCEEDS its committed baseline (scripts/ratchet_baseline.txt). Counts
# may only go down; lowering a baseline is done by editing the baseline
# file in the same commit that deletes the counted code, so the drop is
# visible in review. Raising a baseline is never acceptable.
#
# Metrics:
#   egui_files   files under crates/core/src that reference `egui::`
#   state_bypass ctx.data*/animate_* call sites outside backend/ + memory.rs
#   egui_ui_fns  `&mut egui::Ui`-typed fn params outside backend/
#   ui_escapes   raw-egui escape call sites outside backend/: direct
#                ui_mut()/backend.ui() plus MaraUi::egui_ui[_ref]()
#                (counting the consolidated helper's callers keeps the
#                number honest — hiding escapes behind it can't game it)
set -euo pipefail
cd "$(dirname "$0")/.."

CORE=crates/core/src
BASELINE_FILE=scripts/ratchet_baseline.txt

live_egui_files()   { grep -rl 'egui::' "$CORE" --include='*.rs' | wc -l; }
live_state_bypass() { grep -rEn 'ctx\.data\(|ctx\.data_mut\(|ctx\.animate_' "$CORE" --include='*.rs' | grep -v "$CORE/backend/" | grep -v "$CORE/memory.rs" | wc -l; }
live_egui_ui_fns()  { grep -rEn ':[[:space:]]*&mut egui::Ui' "$CORE" --include='*.rs' | grep -v "$CORE/backend/" | wc -l; }
live_ui_escapes()   { grep -rEn '\.ui_mut\(\)|backend\.ui\(\)|\.egui_ui\(\)|\.egui_ui_readonly\(\)' "$CORE" --include='*.rs' | grep -v "$CORE/backend/" | wc -l; }

declare -A baseline
while read -r key value; do
    [[ -z "$key" || "$key" == \#* ]] && continue
    baseline[$key]=$value
done < "$BASELINE_FILE"

fail=0
check() {
    local name=$1 live=$2 base=${baseline[$1]:-}
    if [[ -z "$base" ]]; then
        echo "ratchet: no baseline for '$name' in $BASELINE_FILE" >&2
        fail=1
        return
    fi
    if (( live > base )); then
        echo "ratchet FAIL: $name = $live (baseline $base) — new egui coupling added" >&2
        fail=1
    elif (( live < base )); then
        echo "ratchet: $name = $live < baseline $base — lower the baseline in this commit"
        fail=1
    else
        echo "ratchet ok: $name = $live"
    fi
}

check egui_files   "$(live_egui_files)"
check state_bypass "$(live_state_bypass)"
check egui_ui_fns  "$(live_egui_ui_fns)"
check ui_escapes   "$(live_ui_escapes)"
exit $fail
