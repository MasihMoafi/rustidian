#!/usr/bin/env python3
"""Check bundled emoji glyphs with FontTools/HarfBuzz; this does not test egui."""
import argparse
import ctypes as c
import ctypes.util
import hashlib
import io
import json
import tempfile
from pathlib import Path

from fontTools.ttLib import TTFont
from PIL import Image

ROOT = Path(__file__).resolve().parents[1]
FONTS = ROOT / "assets/fonts"


def reconstruct_font(destination, provenance):
    packaging = provenance["packaging"]
    directory = FONTS / packaging["directory"]
    parts = sorted(directory.glob("*.part"))
    expected = packaging["parts"]
    assert len(parts) == packaging["part_count"] == len(expected)
    assert [part.name for part in parts] == [f"{index:03}.part" for index in range(len(parts))]
    with destination.open("wb") as output:
        for part, entry in zip(parts, expected):
            data = part.read_bytes()
            assert part.name == entry["file"]
            assert len(data) == entry["bytes"] <= packaging["maximum_part_bytes"]
            assert hashlib.sha256(data).hexdigest() == entry["sha256"], part.name
            output.write(data)
    assert destination.stat().st_size == provenance["bytes"]
    assert hashlib.sha256(destination.read_bytes()).hexdigest() == provenance["sha256"]
    return destination


class GlyphInfo(c.Structure):
    _fields_ = [(name, c.c_uint32) for name in ("glyph", "mask", "cluster", "var1", "var2")]


class GlyphPosition(c.Structure):
    _fields_ = [(name, c.c_int32) for name in ("x_advance", "y_advance", "x_offset", "y_offset", "var")]


class Shaper:
    def __init__(self, path):
        self.hb = hb = c.CDLL(ctypes.util.find_library("harfbuzz"))
        signatures = {
            "hb_blob_create_from_file_or_fail": ([c.c_char_p], c.c_void_p),
            "hb_face_create": ([c.c_void_p, c.c_uint], c.c_void_p),
            "hb_font_create": ([c.c_void_p], c.c_void_p),
            "hb_ot_font_set_funcs": ([c.c_void_p], None),
            "hb_buffer_create": ([], c.c_void_p),
            "hb_buffer_add_utf8": ([c.c_void_p, c.c_char_p, c.c_int, c.c_uint, c.c_int], None),
            "hb_buffer_guess_segment_properties": ([c.c_void_p], None),
            "hb_shape": ([c.c_void_p, c.c_void_p, c.c_void_p, c.c_uint], None),
            "hb_buffer_get_length": ([c.c_void_p], c.c_uint),
            "hb_buffer_get_glyph_infos": ([c.c_void_p, c.POINTER(c.c_uint)], c.POINTER(GlyphInfo)),
            "hb_buffer_get_glyph_positions": ([c.c_void_p, c.POINTER(c.c_uint)], c.POINTER(GlyphPosition)),
            "hb_buffer_destroy": ([c.c_void_p], None),
            "hb_font_destroy": ([c.c_void_p], None),
            "hb_face_destroy": ([c.c_void_p], None),
            "hb_blob_destroy": ([c.c_void_p], None),
            "hb_version_string": ([], c.c_char_p),
        }
        for name, (arguments, result) in signatures.items():
            function = getattr(hb, name)
            function.argtypes, function.restype = arguments, result
        self.blob = hb.hb_blob_create_from_file_or_fail(str(path).encode())
        assert self.blob, path
        self.face = hb.hb_face_create(self.blob, 0)
        self.font = hb.hb_font_create(self.face)
        hb.hb_ot_font_set_funcs(self.font)
        self.tt = TTFont(path)

    def shape(self, text):
        hb = self.hb
        buffer = hb.hb_buffer_create()
        try:
            encoded = text.encode()
            hb.hb_buffer_add_utf8(buffer, encoded, len(encoded), 0, len(encoded))
            hb.hb_buffer_guess_segment_properties(buffer)
            hb.hb_shape(self.font, buffer, None, 0)
            count = c.c_uint(hb.hb_buffer_get_length(buffer))
            infos = hb.hb_buffer_get_glyph_infos(buffer, c.byref(count))
            positions = hb.hb_buffer_get_glyph_positions(buffer, c.byref(count))
            return [dict(glyph=infos[i].glyph, name=self.tt.getGlyphName(infos[i].glyph),
                         cluster=infos[i].cluster, advance=positions[i].x_advance)
                    for i in range(count.value)]
        finally:
            hb.hb_buffer_destroy(buffer)

    def close(self):
        self.hb.hb_font_destroy(self.font)
        self.hb.hb_face_destroy(self.face)
        self.hb.hb_blob_destroy(self.blob)
        self.tt.close()


def color_glyph(shaper, text):
    shaped = shaper.shape(text)
    assert all(glyph["glyph"] != 0 for glyph in shaped), (text, "missing glyph", shaped)
    visible = [glyph for glyph in shaped if glyph["advance"] != 0]
    assert len(visible) == 1, (text, "unjoined sequence", shaped)
    glyph = visible[0]
    assert "CBDT" in shaper.tt, (text, "no color bitmaps")
    bitmap = shaper.tt["CBDT"].strikeData[0][glyph["name"]]
    image = Image.open(io.BytesIO(bitmap.imageData)).convert("RGBA")
    assert any(a > 100 and max(r, g, b) - min(r, g, b) > 30 for r, g, b, a in image.getdata()), text
    return dict(text=text, shaped=shaped, bitmap_size=list(image.size),
                bitmap_sha256=hashlib.sha256(bitmap.imageData).hexdigest())


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    provenance = json.loads((FONTS / "NotoColorEmoji.provenance.json").read_text())
    with tempfile.TemporaryDirectory(prefix="rust-notes-emoji-font-") as temporary:
        path = reconstruct_font(Path(temporary) / "NotoColorEmoji.ttf", provenance)
        check_glyphs(path, provenance, args.output)


def check_glyphs(path, provenance, output):
    emoji, before = Shaper(path), Shaper(FONTS / "Inter-Regular.otf")
    try:
        # Demonstrate that the actual-glyph detector rejects the prior primary font.
        try:
            color_glyph(before, "📂")
        except AssertionError as failure:
            negative = str(failure)
        else:
            raise AssertionError("Missing-font negative unexpectedly passed")
        cases = ["📂", "⚡", "😀", "❤️", "👩‍💻", "👍🏽", "🇮🇷", "1️⃣", "#️⃣", "*️⃣", "🏳️‍🌈"]
        glyphs = [color_glyph(emoji, text) for text in cases]
        markers = json.loads((FONTS / "emoji-markers/provenance.json").read_text())
        for marker in markers["glyphs"]:
            png = (FONTS / "emoji-markers" / f'{marker["color"]}.png').read_bytes()
            bitmap = emoji.tt["CBDT"].strikeData[0][marker["glyph"]]
            assert png == bitmap.imageData, marker["color"]
            assert hashlib.sha256(png).hexdigest() == marker["sha256"]
        evidence = dict(
            boundary="Asset-only check with system HarfBuzz and FontTools, not Rust/harfrust/egui or native GUI verification",
            font_sha256=provenance["sha256"], harfbuzz=emoji.hb.hb_version_string().decode(),
            packaged_parts_verified=provenance["packaging"]["part_count"],
            missing_font_negative=negative, actual_color_glyphs=glyphs, unchanged_marker_pngs=6,
        )
        if output:
            output.parent.mkdir(parents=True, exist_ok=True)
            output.write_text(json.dumps(evidence, ensure_ascii=False, indent=2) + "\n")
        print(json.dumps(dict(negative_rejected=True, actual_color_sequences=len(glyphs),
                              unchanged_marker_pngs=6, boundary=evidence["boundary"])))
    finally:
        emoji.close()
        before.close()


if __name__ == "__main__":
    main()
