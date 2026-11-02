# Pixel diff of two shot trees: python pixdiff.py <before> <after> <out_dir>
# Prints per set (size/mode) the number of screens, changed screens and changed pixels; writes diff/crop images for any
# non-zero pair into <out_dir> and a JSON summary.
import json, os, sys
import numpy as np
from PIL import Image

before, after, out = sys.argv[1:4]
os.makedirs(out, exist_ok=True)
summary = {}
for size in sorted(os.listdir(before)):
    for mode in sorted(os.listdir(os.path.join(before, size))):
        key = f"{size}/{mode}"
        rows = []
        for f in sorted(os.listdir(os.path.join(before, size, mode))):
            a = Image.open(os.path.join(before, size, mode, f)).convert("RGB")
            pb = os.path.join(after, size, mode, f)
            if not os.path.exists(pb):
                rows.append({"screen": f[:-4], "missing": True}); continue
            b = Image.open(pb).convert("RGB")
            if a.size != b.size:
                h = max(a.height, b.height); w = max(a.width, b.width)
                A = np.zeros((h, w, 3), np.uint8); B = np.zeros((h, w, 3), np.uint8)
                A[:a.height, :a.width] = np.asarray(a); B[:b.height, :b.width] = np.asarray(b)
            else:
                A, B = np.asarray(a), np.asarray(b)
            mask = np.any(A != B, axis=2)
            n = int(mask.sum())
            row = {"screen": f[:-4], "changed": n, "size_before": a.size, "size_after": b.size}
            if n:
                ys, xs = np.nonzero(mask)
                box = (max(0, xs.min() - 40), max(0, ys.min() - 40), min(A.shape[1], xs.max() + 41), min(A.shape[0], ys.max() + 41))
                row["bbox"] = [int(v) for v in box]
                tag = f"{size}_{mode}_{f[:-4]}"
                Image.fromarray(A).crop(box).save(os.path.join(out, f"{tag}_before.png"))
                Image.fromarray(B).crop(box).save(os.path.join(out, f"{tag}_after.png"))
                d = np.zeros_like(A); d[mask] = [255, 0, 0]
                Image.fromarray(d).crop(box).save(os.path.join(out, f"{tag}_diff.png"))
            rows.append(row)
        summary[key] = rows
        changed = [r for r in rows if r.get("changed") or r.get("missing")]
        print(f"{key:24} screens={len(rows)} changed_screens={len(changed)} changed_pixels={sum(r.get('changed', 0) for r in rows)}")
json.dump(summary, open(os.path.join(out, "summary.json"), "w"), indent=1)
