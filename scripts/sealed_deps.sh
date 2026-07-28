#!/usr/bin/env bash
# Sealed-tier dependency gate — PLAN.md WS-G2.
#
# Asserts that no sealed crate has a DIRECT dependency on a backend:
# egui, any egui-* sibling, wgpu, or `mara_backend_egui` (depending on
# the backend crate reaches egui without ever writing the token).
#
# Structural rather than textual. `cargo metadata` reports each
# dependency's *resolved package name*, so this catches a renamed dep —
#
#     ui_kit = { package = "egui", version = "0.34" }
#
# — which the manifest grep in the Makefile is blind to, because the
# line it greps for never appears. That hole was demonstrated before
# this script was written, not hypothesised.
#
# Why direct edges and not `cargo tree -e normal -i egui`: every sealed
# crate depends on `mara_core`, and `mara_core` pulls egui in through its
# default-on `backend-egui-conv` conversion feature. A transitive query
# therefore flags every module in the workspace for something that is not
# theirs. The transitive claim that IS meaningful — `mara_core` itself
# has no egui edge with the feature off — is asserted separately in
# `make check` with `cargo tree`.
set -euo pipefail

cd "$(dirname "$0")/.."

# `mara_graph` is the last sealed-tier exception, pending its WS-D1
# split. Named here rather than silently skipped so the exemption is
# visible in the failure output.
EXEMPT="mara_graph"

BANNED_RE='^(egui|egui[-_][a-z0-9]+|wgpu|mara_backend_egui)$'

metadata=$(cargo metadata --no-deps --format-version 1)

violations=$(
    echo "$metadata" | jq -r --arg banned "$BANNED_RE" --arg exempt "$EXEMPT" '
        .packages[]
        | select(.manifest_path | test("/crates/modules/"))
        | select(.name as $n | ($exempt | split(" ") | index($n)) | not)
        | .name as $crate
        | .dependencies[]
        | select(.kind == null)
        | select(.name | test($banned))
        | "\($crate) -> \(.name)"
    '
)

if [ -n "$violations" ]; then
    echo "sealed tier: direct backend dependency" >&2
    echo "$violations" | sed 's/^/  /' >&2
    echo >&2
    echo "A crate in crates/modules/ names no backend type and depends on" >&2
    echo "no backend crate. Renderer-owning crates belong in hosts/." >&2
    exit 1
fi

echo "sealed deps ok: no direct backend edge in crates/modules/ (exempt: $EXEMPT)"
