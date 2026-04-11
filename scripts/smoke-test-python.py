"""
Smoke test for the chordsketch Python package.

Called from .github/workflows/python.yml by each per-OS test job
(test-ubuntu, test-macos, test-windows) after the wheel has been
installed, so that the test logic lives in a single place.
"""

import chordsketch

v = chordsketch.version()
assert v, "version should not be empty"
print(f"Version: {v}")

text = chordsketch.parse_and_render_text("{title: Test}\n[C]Hello", None, None)
assert "Test" in text and "Hello" in text
print("Text render: OK")

html = chordsketch.parse_and_render_html("{title: Test}\n[C]Hello", None, None)
assert "Test" in html
print("HTML render: OK")

pdf = chordsketch.parse_and_render_pdf("{title: Test}\n[C]Hello", None, None)
assert pdf[:4] == b"%PDF"
print("PDF render: OK")

errors = chordsketch.validate("{title: Valid}\n[C]Hello")
assert isinstance(errors, list)
print("Validate: OK")

text = chordsketch.parse_and_render_text("{title: Test}\n[C]Hello", "guitar", None)
assert "Test" in text
print("Config preset: OK")

chordsketch.parse_and_render_text("{title: Test}\n[C]Hello", None, 2)
print("Transpose: OK")

print("All smoke tests passed!")
