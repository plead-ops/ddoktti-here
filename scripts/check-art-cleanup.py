"""Regression checks for colour cleanup; run with the artwork generation venv."""
import importlib.util
from pathlib import Path
from PIL import Image, ImageDraw

spec=importlib.util.spec_from_file_location("trace_pet",Path(__file__).with_name("trace-pet-svg.py"))
module=importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)

im=Image.new("RGBA",(50,60),(0,0,0,0))
draw=ImageDraw.Draw(im)
# Ambiguous near-black pixels on the antenna/rim must become uniform ink.
draw.rectangle((10,5,20,12),fill=(20,35,50,255))
# The same dark shading inside a connected coat must remain flat cloth.
draw.rectangle((8,25,42,55),fill=(38,55,79,255))
draw.rectangle((15,35,25,45),fill=(20,35,50,255))
# A genuine structural seam remains black, not erased with the shading.
draw.rectangle((32,30,34,50),fill=(8,23,36,255))
clean=module.clean_pixels(im,suit_from=25)
assert clean.getpixel((15,8))==(8,23,36,255), "rim colour broke into suit-coloured fragments"
assert clean.getpixel((20,40))==(38,55,79,255), "coat shading became a black blotch"
assert clean.getpixel((33,40))==(8,23,36,255), "structural seam was erased"
assert clean.getpixel((0,0))[3]==0, "transparent background gained pixels"
# Existing walking/running seams already describe their folds; no detached additions.
assert "walk-0" not in module.REPAIRS["HEMS"]
assert "run-v3-0" not in module.REPAIRS["HEMS"]
print("Artwork cleanup: rim, coat, seam, transparency and detached-line checks passed")

for i in range(20):
    key=f"emotions-v2-{i}"
    assert key in module.REPAIRS["HEMS"]
    result=module.REPAIRS["repair"](key,'<path d="M0 0H10V10Z" fill="#26374F"/>')
    assert f'clip-path="url(#hem-{key})"' in result, "hem can escape jacket silhouette"
assert "MOUTHS" not in module.REPAIRS and "hurt_frames" not in module.REPAIRS
print("All emotion hems clipped to cloth; no manual mouth/body replacement overlays")
