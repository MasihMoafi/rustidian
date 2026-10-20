# Text backend provenance

Rust Notes pins cached `egui` and `epaint` 0.37.0 with local Cargo patches.
Their published `.cargo_vcs_info.json` identifies egui commit
`7c0b40402271e8d40cda69f87cf3c2617c2e4382`.
The other egui ecosystem crates retain their registry versions.

The initial bidi changes come from the draft upstream
[PR 8577](https://github.com/emilk/egui/pull/8577), commits
`c55da0337f67616ee7be4bfb06b73fdd71ef7f91` and
`c93e8863e958507056415e2a6dace56e23ba5f3a`.
The original downloaded patch and separate source patches are in `patches/`.
They address [incorrect RTL glyph/cursor counts](https://github.com/emilk/egui/issues/8576).
This is a local port of an unfinished upstream change.

Port adaptations: retain the 0.37 font imports and raster constructor;
add the `bidi_level` initializer there; relocate the added composed-glyph test
after the current test helper. `epaint` adds cached `unicode-bidi` 0.3.18 with
hardcoded Unicode data and no default features. No Cargo registry files changed.

Local corrections after reproducing failures:

- Keep advancing Arabic bases before zero-width combining marks and preserve
  each glyph entry's logical source character.
- Place a mixed row's final caret at its final logical glyph edge.
- Paint disjoint visual selection rectangles, including IME underlines,
  rather than highlighting intervening unselected RTL characters.
- Resolve runs in an LTR paragraph, matching the reference editor's default
  CSS direction and keeping Markdown markers/punctuation in that context.

Application tests exercise glyph counts, original Unicode, mark ownership,
mixed-row end carets, real selection painting and the paragraph base direction.
The GUI test uses disposable vaults and checks exact Persian save/undo bytes.
Font licenses are maintained separately in `assets/fonts/`.

Native editing folds also retain logical characters in zero-height rows.
Local galley hit testing and horizontal/vertical navigation skip those rows;
an application galley test and real Source/Live GUI checks exercise this.
This does not change the unresolved visual bidi cursor affinity described below.

Remaining limitations: arrow movement and word navigation use egui's logical
cursor model; complete visual bidi affinity, trailing-whitespace rule L1,
and actual desktop IME composition have not been established. These changes
do not establish complete RTL editor parity. The draft patch's original
auto-detected paragraph-direction tests also differ from this app's LTR policy.

`LICENSE-MIT` and `LICENSE-APACHE` were retrieved from the official egui
repository's main branch because the published source commit URL returned 404.
The crates declare `MIT OR Apache-2.0`; retain both license files when distributing.


21 October local folding optimization: `Row::clone_without_visuals` copies logical glyphs and layout metadata without copying the rendered vertex/index buffers. Rust Notes uses it only for zero-height folded rows, whose visuals are empty. Visible rows and the retained original font galley stay unchanged. Logical cursor/selection data survives, and the tessellator can skip empty hidden meshes even without coarse culling. This extension is local, not an upstream claim; patch: `patches/fold-row-without-visuals.patch`. The native editor regression asserts hidden meshes are absent, glyphs preserved and visible visuals unchanged, and demonstrably fails before the fix.
