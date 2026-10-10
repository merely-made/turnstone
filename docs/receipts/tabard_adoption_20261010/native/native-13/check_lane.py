#!/usr/bin/env python3
"""CPU-only exact evidence checks over a completed isolated native lane."""
import argparse
from collections import Counter
import hashlib
import json
from pathlib import Path
import re
import struct
import zlib

BASE = Path(__file__).resolve().parent
WORKTREE = Path('/Users/markik/Code/worktrees/turnstone-tabard-apps')
BINARY = Path('/Users/markik/Code/targets/knot-tabard-apps/debug/turnstone')

def sha(path):
    h = hashlib.sha256()
    with path.open('rb') as f:
        for block in iter(lambda: f.read(1024 * 1024), b''):
            h.update(block)
    return h.hexdigest()

def rgba(path):
    raw = path.read_bytes()
    assert raw[:8] == b'\x89PNG\r\n\x1a\n'
    parts = []
    offset = 8
    while offset < len(raw):
        size = struct.unpack_from('>I', raw, offset)[0]
        kind = raw[offset+4:offset+8]
        value = raw[offset+8:offset+8+size]
        offset += size + 12
        if kind == b'IHDR':
            w,h,depth,color,_,_,interlace = struct.unpack('>IIBBBBB', value)
        if kind == b'IDAT':
            parts.append(value)
    assert (depth,color,interlace) == (8,6,0)
    data = zlib.decompress(b''.join(parts))
    stride = w * 4
    previous = bytearray(stride)
    result = bytearray()
    for y in range(h):
        pos = y * (stride+1)
        filter_type = data[pos]
        row = bytearray(data[pos+1:pos+1+stride])
        if filter_type == 1:
            for j in range(4,stride):
                row[j] = (row[j] + row[j-4]) & 255
        elif filter_type == 2:
            row = bytearray((a+b)&255 for a,b in zip(row,previous))
        elif filter_type in (3,4):
            for j in range(stride):
                a = row[j-4] if j>=4 else 0
                b = previous[j]
                c = previous[j-4] if j>=4 else 0
                if filter_type == 3:
                    predict = (a+b)//2
                else:
                    p = a+b-c
                    pa,pb,pc = abs(p-a),abs(p-b),abs(p-c)
                    predict = a if pa<=pb and pa<=pc else b if pb<=pc else c
                row[j] = (row[j]+predict)&255
        else:
            assert filter_type == 0
        result.extend(row)
        previous = row
    return w,h,result

def actual_preview_rect(w, h, data, bg):
    # The authored background uniquely marks the isolated application preview.
    # Long exact-color horizontal runs identify its rendered outer extent even
    # when the workshop scrolls; the six role requirements below stay unchanged.
    left = w // 4 if w == 2360 else 0
    pattern = re.compile(b"(?:" + re.escape(bytes(bg)) + b"){" + str(w // 3).encode() + b",}")
    runs = []
    for y in range(h):
        row = bytes(data[(y*w+left)*4:((y+1)*w)*4])
        for match in pattern.finditer(row):
            assert match.start() % 4 == 0 and match.end() % 4 == 0
            runs.append((left + match.start() // 4, y, left + match.end() // 4))
    assert runs, "authored document preview boundary not found"
    box = (min(r[0] for r in runs), min(r[1] for r in runs),
           max(r[2] for r in runs), max(r[1] for r in runs) + 1)
    assert box[2] - box[0] > w // 2 and box[3] - box[1] > 400, box
    return box

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--lane', choices=['seed','reopen','same-id','same-id-reopen'], required=True)
    lane = parser.parse_args().lane
    freeze = json.loads((BASE/'source-manifest.json').read_text())
    assert sha(BINARY) == freeze['binary_sha256']
    for name,digest in freeze['build_input_sha256'].items():
        assert sha(WORKTREE/name) == digest, name
    theme = json.loads((BASE/'library/themes.json').read_text())['themes'][0]
    assert theme['id']=='theme:copy-1' and theme['name']=='TurnstoneAcceptance'
    edited = lane in ['same-id','same-id-reopen']
    fixture = BASE/'scenarios'/('tabard_workshop_same_id.scn' if edited else 'tabard_workshop.scn')
    css = re.search(r':root\{[^\n]*\}', fixture.read_text()).group(0)
    assert theme['mode_sheets']['dark'][0] == css
    settings = json.loads((BASE/'turnstone-profile/application/settings.json').read_text())
    choice = {key:settings[key] for key in ['theme_id','theme_mode']}
    assert choice == {'theme_id':'theme:copy-1','theme_mode':'dark'}
    facts = {'theme_id':theme['id'],'name':theme['name'],'css_utf8_bytes':len(css.encode()),
      'css_sha256':hashlib.sha256(css.encode()).hexdigest(),'css_exact_fixture_bytes':True,
      'choice':choice,'private_library_dump_included':False}
    images = []
    for path in sorted((BASE/lane).glob('**/*.png')):
        w,h,data = rgba(path)
        counts = Counter(struct.iter_unpack('4B',data))
        assert len(counts)>100 and counts.most_common(1)[0][0][3]==255
        entry = {'file':str(path.relative_to(BASE/lane)),'size':[w,h],
          'sha256':sha(path),'distinct_rgba':len(counts),'dominant_rgba':counts.most_common(8)}
        if path.parent.name == 'app':
            offset = ((h//2)*w + w-40)*4
            pixel = tuple(data[offset:offset+4])
            name = path.name
            expected = None
            if lane in ['reopen','same-id-reopen']:
                expected = (52,34,20,255) if edited else (20,34,56,255)
            elif lane == 'same-id':
                expected = (20,34,56,255) if 'before' in name or 'saved_unapplied' in name else (52,34,20,255)
            elif name in ['app_dark.png','app_authored_narrow.png']:
                expected = (20,34,56,255)
            if expected is not None:
                assert pixel==expected,(name,pixel,expected)
            entry['background_probe_rgba'] = pixel
        else:
            bg = (52,34,20,255) if edited else (20,34,56,255)
            box = actual_preview_rect(w, h, data, bg)
            x0,y0,x1,y1 = box
            roi = b''.join(data[(y*w+x0)*4:(y*w+x1)*4] for y in range(y0,y1))
            colors = Counter(struct.iter_unpack('4B',roi))
            assert len(colors)>1000
            bg = (52,34,20,255) if edited else (20,34,56,255)
            assert colors[bg]>10000
            for role in [(27,46,73,255),(36,61,94,255),(47,127,255,255),(156,213,255,255),(234,242,255,255)]:
                assert colors[role]>100,(path.name,role,colors[role])
            entry['preview_roi'] = {'box':box,'geometry_method':'exact authored background horizontal runs >=image_width/3, checked against independent original-image review','distinct_rgba':len(colors),'colors':colors.most_common(12),'six_exact_role_counts':{'#%02x%02x%02x'%role[:3]:colors[role] for role in [bg,(27,46,73,255),(36,61,94,255),(47,127,255,255),(156,213,255,255),(234,242,255,255)]}}
        images.append(entry)
        print(path.name,'colors',len(counts),'PASS',flush=True)
    output = {'lane':lane,'decoder':'independent stdlib PNG/zlib RGBA scanline unfilter',
      'exact_facts':facts,'images':images,'frozen_source_and_binary_unchanged':True,
      'visual_inspection':'separate original-image review required'}
    (BASE/lane/'pixel-acceptance.json').write_text(json.dumps(output,indent=2)+'\n')
    print(json.dumps(facts),flush=True)

if __name__ == '__main__':
    main()
