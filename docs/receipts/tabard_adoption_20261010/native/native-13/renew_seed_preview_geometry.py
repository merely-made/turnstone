#!/usr/bin/env python3
"""CPU-only geometry renewal on original seed images; preserves initial facts."""
from pathlib import Path
from collections import Counter
import json, struct
from check_lane import rgba, actual_preview_rect, sha
BASE=Path(__file__).resolve().parent
p=BASE/'seed/pixel-acceptance.json'
d=json.loads(p.read_text())
backup=BASE/'seed/fixed-roi-initial-pixel-acceptance.json'
assert not backup.exists()
backup.write_text(p.read_text())
roles=[(20,34,56,255),(27,46,73,255),(36,61,94,255),(47,127,255,255),(156,213,255,255),(234,242,255,255)]
for entry in d['images']:
    if not entry['file'].startswith('workshop/'):
        continue
    image=BASE/'seed'/entry['file']
    assert sha(image)==entry['sha256']
    w,h,data=rgba(image)
    box=actual_preview_rect(w,h,data,roles[0])
    x0,y0,x1,y1=box
    counts=Counter()
    for y in range(y0,y1):
        counts.update(struct.iter_unpack('4B',data[(y*w+x0)*4:(y*w+x1)*4]))
    assert len(counts)>1000 and counts[roles[0]]>10000
    for role in roles[1:]:assert counts[role]>100
    entry['preview_roi']={'box':box,'geometry_method':'exact authored background horizontal runs >=image_width/3, checked against independent original-image review','distinct_rgba':len(counts),'colors':counts.most_common(12),'six_exact_role_counts':{'#%02x%02x%02x'%role[:3]:counts[role] for role in roles}}
    print(entry['file'],box,entry['preview_roi']['six_exact_role_counts'],flush=True)
d['geometry_renewal']='existing original seed PNGs only; prior initial library/choice/source facts remain their separately completed pre-edit check; no native rerun or product source change'
p.write_text(json.dumps(d,indent=2)+'\n')
