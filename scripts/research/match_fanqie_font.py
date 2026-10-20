"""Reproduce the glyph mapping; requires fonttools==4.67.0, no network access.

Inputs are locally supplied fonts. No font binaries or novel content are emitted.
"""
import argparse
import hashlib
import json
from pathlib import Path

from fontTools.pens.recordingPen import RecordingPen
from fontTools.ttLib import TTFont


def outline(glyph):
    pen = RecordingPen()
    glyph.draw(pen)
    return tuple(
        (operator, tuple(tuple(round(float(v), 4) for v in point) for point in points))
        for operator, points in pen.value
    )


def match(source: Path, reference: Path):
    subset = TTFont(source)
    original = TTFont(reference)
    glyphs = original.getGlyphSet()
    candidates = {}
    for codepoint, name in original.getBestCmap().items():
        if 32 <= codepoint <= 126 or 0x4E00 <= codepoint <= 0x9FFF:
            candidates.setdefault(outline(glyphs[name]), set()).add(codepoint)
    glyphs = subset.getGlyphSet()
    mapping = {}
    for codepoint, name in subset.getBestCmap().items():
        matches = candidates.get(outline(glyphs[name]), set())
        if len(matches) != 1:
            raise ValueError(f"Glyph {codepoint} has {len(matches)} matches; refusing an incomplete map")
        mapping[str(codepoint)] = next(iter(matches))
    return {
        "font_sha256": hashlib.sha256(source.read_bytes()).hexdigest(),
        "reference_sha256": hashlib.sha256(reference.read_bytes()).hexdigest(),
        "mapping": mapping,
        "provenance": "Exact unique glyph-outline matching against Adobe SourceHanSansSC-Normal 2.002R; see docs/research/fanqie-web.md.",
    }


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("source", type=Path)
    parser.add_argument("reference", type=Path)
    args = parser.parse_args()
    print(json.dumps(match(args.source, args.reference), ensure_ascii=False, indent=2))
