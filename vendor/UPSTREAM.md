# Text backend provenance

Rustidian pins cached `egui` and `epaint` 0.37.0 with local Cargo patches.
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
- Keep a section's leading space when a row break lands just before it,
  so a Live preview inline formula keeps its reserved width after a wrap
  (`patches/wrap-keeps-leading-space.patch`).

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


21 October local folding optimization: `Row::clone_without_visuals` copies logical glyphs and layout metadata without copying the rendered vertex/index buffers. Rustidian uses it only for zero-height folded rows, whose visuals are empty. Visible rows and the retained original font galley stay unchanged. Logical cursor/selection data survives, and the tessellator can skip empty hidden meshes even without coarse culling. This extension is local, not an upstream claim; patch: `patches/fold-row-without-visuals.patch`. The native editor regression asserts hidden meshes are absent, glyphs preserved and visible visuals unchanged, and demonstrably fails before the fix.

22 October color emoji: application font families now append licensed Noto Color
Emoji and enable epaint's native color-font support. Local fallback resolution
selects an emoji-presented grapheme before the ordinary character cache and keeps
a separate emoji cache, so plain heart/digit layout cannot suppress a later
VS16/keycap sequence. Ordinary ASCII fallback skips nonprimary color glyphs:
otherwise # and * in the Arabic-only family acquire wider Noto keycap bases.
The exact app-font metric regression failed in Actions39758705958; it remains
unchanged to verify the repair. Emoji sequences, original marker pixels and
text presentation retain actual shaping/atlas tests. This is a local correction,
not an upstream release claim.

Typing performance (27 Oct): `TextEdit` events no longer lay out the galley after
each typed character, only before an event that reads it (cursor keys, IME,
AccessKit) and once after the batch. Rustidian's layouter reparses Markdown, so a
frame of queued keystrokes in an 86 KB note cost one full layout per character.
The press and release of a character key around its text (no modifier but Shift,
not a cursor, editing or return key) do not read it either; laying out before
them still cost one layout per character (64 characters: a 500 ms frame).

Incremental paragraph layout (27 Oct): `FontsView::layout_job_incremental` lays a
job out paragraph by paragraph like `layout_job`'s split path, but takes the
`ParagraphLayout` it returned last time and reuses the galley of every paragraph
whose text and sections are unchanged, without hashing the whole job or the
unchanged paragraphs. `FontsView::layout_generation` changes whenever a new
`Fonts` is built or the galley caches are cleared, so an application holding
galleys across passes knows when they stopped being valid. Rustidian's editor
uses both to lay out one paragraph per keystroke in a large note; a crate test
compares the result of random edits with `layout_job` and checks paragraph reuse.

Wrapped quote insets (31 Oct 2026)

`FontsView::layout_job_incremental_indented` reserves an optional left inset per
paragraph before wrapping and retains it on every row. Indents participate in
paragraph reuse; the existing method keeps zero insets. Rustidian uses this for
quotes so narrow wrapped text cannot cross its border or its right edge.
