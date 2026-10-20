# Generated native icons

Raster artwork was made with the session's built-in `image_gen.imagegen` tool. No Codex CLI or separate image API was invoked. The underlying image model was not independently identified.

The workflow follows Masih's [logo skill](../../../skills/logo/SKILL.md): preserve images and prompts, select artwork, convert with SVG Studio's multicolour engine, verify genuine paths, and inspect source/vector proofs before native integration.

## Selection and conversion

- `artwork/candidate-{a,b,c}.png` and their prompts preserve three generated sheets. A was selected for its lighter rounded line work; B is angular and C heavier.
- The ambiguous folder-shaped new-tab glyph was replaced with separately generated `new-tab-original.png`. `more-original.png` supplies the three-dot note-actions menu. Both prompts are saved.
- `scripts/build-icons.mjs` extracts equal atlas cells, removes transparent margins, fits artwork proportionally into a standard viewport, then calls SVG Studio's `vectorizeMulticolourImage` with palette 2 and background removal disabled.
- `artwork/source-*.png` are normalized inputs; `artwork/traced-*.svg` are unchanged trace results. Native SVGs have only fills changed to white for egui tinting. Paths, holes, and viewBoxes are preserved.
- `manifest.json` records engine, settings, sources, adjustments, path counts, and output sizes. The native assets contain genuine paths, without embedded rasters, fonts, or active content.

## Icon meanings

| Asset | Meaning |
| --- | --- |
| `files.svg`, `search.svg` | File explorer; vault search |
| `new-note.svg` | Create a Markdown note |
| `new-tab.svg` | Empty tab; creates no file |
| `sidebar-left.svg`, `sidebar-right.svg` | Toggle the corresponding sidebar |
| `back.svg`, `forward.svg` | Previous/next visited note |
| `reading.svg`, `edit.svg` | Enter reading/source mode |
| `more.svg` | Note-actions menu |
| `commands.svg` | Command palette |
| `outline.svg` | Decorative outline heading, not a button |
| `split.svg`, `save.svg`, `vault.svg`, `appearance.svg` | Corresponding named actions; these remain accessible through labeled menus/commands rather than ambiguous primary buttons |

## Property icons

Eight property glyphs were generated with the same built-in image tool after measuring Obsidian's properties layout. `artwork/property-sheet-original.png` and its prompt retain the selected image; the first transparent sheet was rejected for visible noise and is retained with that reason. The clean white canvas was removed by luminance coverage, then each glyph was cropped and traced through the same SVG Studio module. Only explicit fill colours change for native tinting. `manifest.json` records the preprocessing and all selected assets. The proof includes all eight property icons and their counters at native UI sizes. The logo skill's six pipeline regression checks pass.

## Explorer and disclosure icons

New Folder, Sort, Collapse All and the disclosure chevron were generated as images and traced through SVG Studio. The Files tab uses a newly generated plain folder. Originals, exact prompts, selection reasons and preprocessing are retained under artwork. There are31 distinct selected assets; reports/icon-reproduction-31.json verifies byte-for-byte reproduction of every native SVG from its selected generated source.

Close and Copy use the same generated-image workflow. Their selected monochrome SVG Studio conversion retains the generated rounded corners; two earlier multicolour proof sheets were rejected for flattening them. The prepared raster is768px and the engine’s10px padding is cropped through viewBox only. Exact settings and original artwork are retained.

## Proof and reproduction

`proof/source-vs-vector.html` compares all traces at 24/48/128/512px on light/dark surfaces with full-frame and foreground measurements. `proof/native-tints.png` compares silhouettes with the actual native tinting at 12/18/24/48px. Both were inspected for counters, silhouettes, margins, speckles, and legibility, followed by native-window review in both themes. Rust tests reject duplicated artwork across distinct icon identities and embedded rasters/text/scripts/foreign objects.

```sh
SVG_STUDIO_MODULE=/home/masih/Desktop/p/svg-wt-multicolour/mcp-server/dist/vectorize.js node scripts/build-icons.mjs
SVG_STUDIO_MODULE=/home/masih/Desktop/p/svg-wt-multicolour/mcp-server/dist/vectorize.js node scripts/build-atlas-icons.mjs properties
SVG_STUDIO_MODULE=/home/masih/Desktop/p/svg-wt-multicolour/mcp-server/dist/vectorize.js node scripts/build-atlas-icons.mjs file-toolbar
SVG_STUDIO_MODULE=/home/masih/Desktop/p/svg-wt-multicolour/mcp-server/dist/vectorize.js node scripts/build-atlas-icons.mjs files-tab
SVG_STUDIO_MODULE=/home/masih/Desktop/p/svg-wt-multicolour/mcp-server/dist/vectorize.js node scripts/build-atlas-icons.mjs disclosure
SVG_STUDIO_MODULE=/home/masih/Desktop/p/svg-wt-multicolour/mcp-server/dist/vectorize.js node scripts/build-atlas-icons.mjs code-controls
SVG_STUDIO_MODULE=/home/masih/Desktop/p/svg-wt-multicolour/mcp-server/dist/vectorize.js node scripts/prove-icons.mjs
```

Node and SVG Studio prepare artwork; the application itself remains native Rust.


## Knowledge-action artwork — 21 October

Six additional icons (Graph, Canvas, Bookmark, Daily note, Templates, Recovery) were generated with the built-in image tool and traced through the existing SVG Studio monochrome engine. The exact sheet/prompt, prepared 768px rasters, traces, options and paths are retained. Source/SVG pairs were inspected at24/48/128/512px and native tints at12/18/24/48px on both themes; counters, silhouettes and margins are preserved. The six pipeline regressions pass. Reports knowledge-icons-native-proof.png and knowledge-proof-a/b.png contain inspection crops. There are37 selected artworks;31 are currently integrated into the executable and these six are prepared for their explicit feature actions. Do not claim their features are implemented from the artwork alone.

## Callout artwork — 21 October

Thirteen callout symbols were generated with the built-in image tool using the knowledge sheet as a style reference, then traced through SVG Studio's existing monochrome engine. The new sheet/prompt, prepared rasters, unchanged traces and native tint variants are retained. All thirteen source/vector pairs were inspected at24/48/128/512px and native12/18/24/48px in both themes; the six pipeline checks pass. The candidate uses these for the thirteen callout types. Native-window verification remains pending. There are50 selected artworks, with44 referenced by candidate source and six knowledge-action assets still prepared only.

The complete proof is now paginated at30 marks per sheet to stay below the renderer's32767px canvas limit. `proof/source-vs-vector.html` indexes every sheet, and `source-vs-vector.json` retains all measurements. Per-size render crops are intermediate output; the indexed proof sheets are the retained inspection artifact.
