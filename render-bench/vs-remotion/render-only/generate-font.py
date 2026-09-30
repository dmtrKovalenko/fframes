"""Build the benchmark's original 3x5 digit font (requires fonttools).

Integer-aligned outlines let both renderers produce exactly the same pixels.
"""
import json
from pathlib import Path

from fontTools.fontBuilder import FontBuilder
from fontTools.pens.ttGlyphPen import TTGlyphPen

root = Path(__file__).parent
patterns = json.loads((root / "glyphs.json").read_text())
names = [".notdef"] + [f"digit{i}" for i in range(10)]
font = FontBuilder(1000, isTTF=True)
font.setupGlyphOrder(names)
font.setupCharacterMap({ord(str(i)): names[i + 1] for i in range(10)})
glyphs = {}
for name, rows in zip(names, [["000"] * 5] + patterns):
    pen = TTGlyphPen(None)
    for row, bits in enumerate(rows):
        for col, bit in enumerate(bits):
            if bit == "0":
                continue
            x, y = col * 100, (4 - row) * 100
            pen.moveTo((x, y))
            pen.lineTo((x, y + 100))
            pen.lineTo((x + 100, y + 100))
            pen.lineTo((x + 100, y))
            pen.closePath()
    glyphs[name] = pen.glyph()
font.setupGlyf(glyphs)
font.setupHorizontalMetrics({name: (400, 0) for name in names})
font.setupHorizontalHeader(ascent=700, descent=-200)
font.setupNameTable({"familyName": "Bench Digits", "styleName": "Regular",
                    "uniqueFontIdentifier": "BenchDigits-Regular-1",
                    "fullName": "Bench Digits Regular", "psName": "BenchDigits-Regular"})
font.setupOS2(sTypoAscender=700, sTypoDescender=-200, usWinAscent=700, usWinDescent=200)
font.setupPost()
font.setupMaxp()
font.font["head"].created = font.font["head"].modified = 2082844800
font.font.recalcTimestamp = False
font.save(root / "media" / "BenchDigits.ttf")
