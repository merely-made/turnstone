#!/usr/bin/env python3
"""Measure actual textbox borders in reviewed native capture regions; CPU only.
Regions include one complete Theme name or seed field and its label. They are
selected from the original capture, never estimated automation hit points.
"""
import argparse
import hashlib
import json
from pathlib import Path
import struct
import zlib
from check_lane import rgba

def chunk(kind, data):
    return struct.pack('>I',len(data))+kind+data+struct.pack('>I',zlib.crc32(kind+data)&0xffffffff)

def crop_png(path,w,data,box):
    x0,y0,x1,y1 = box
    rows = b''.join(b'\0'+data[(y*w+x0)*4:(y*w+x1)*4] for y in range(y0,y1))
    path.write_bytes(b'\x89PNG\r\n\x1a\n'+chunk(b'IHDR',struct.pack('>IIBBBBB',x1-x0,y1-y0,8,6,0,0,0))+
                    chunk(b'IDAT',zlib.compress(rows))+chunk(b'IEND',b''))

def longest_border(data,w,y,x0,x1):
    best = None
    start = None
    for x in range(x0,x1):
        offset = (y*w+x)*4
        pixel = data[offset:offset+4]
        matches = pixel[3]==255 and all(abs(pixel[j]-c)<=3 for j,c in enumerate((203,211,223)))
        if matches and start is None:
            start=x
        if start is not None and (not matches or x==x1-1):
            end=x if not matches else x+1
            if best is None or end-start>best[1]-best[0]:
                best=(start,end)
            start=None
    return best

def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('--image',type=Path,required=True)
    p.add_argument('--name-region',type=int,nargs=4,required=True)
    p.add_argument('--seed-region',type=int,nargs=4,required=True)
    p.add_argument('--logical-width',type=float,default=1180)
    p.add_argument('--output',type=Path,required=True)
    args=p.parse_args()
    w,h,data=rgba(args.image)
    scale=w/args.logical_width
    assert scale==2.0,('capture scale changed; review before measuring',scale)
    args.output.mkdir(mode=0o700,exist_ok=False)
    records={}
    for name,box in [('name',args.name_region),('seed-hex',args.seed_region)]:
        x0,y0,x1,y1=box
        assert 0<=x0<x1<=w and 0<=y0<y1<=h
        edges=[]
        for y in range(y0,y1):
            run=longest_border(data,w,y,x0,x1)
            if run is not None and run[1]-run[0] >= 0.65*(x1-x0):
                edges.append((y,run))
        assert len(edges)>=2,(name,'complete horizontal field borders not found')
        top,bottom=edges[0][0],edges[-1][0]
        height=bottom-top+1
        assert height>=68,(name,'field collapsed',height)
        assert height<200,(name,'region contains multiple controls; review narrower crop',height)
        crop=args.output/(name+'.png')
        crop_png(crop,w,data,box)
        records[name]={'reviewed_region':box,'border_first_physical_y':top,
          'border_last_physical_y':bottom,'outer_painted_height_physical':height,
          'outer_painted_height_css':height/scale,'minimum_css_height':34,
          'border_rgb_target':[203,211,223],'channel_tolerance':3,
          'crop_sha256':hashlib.sha256(crop.read_bytes()).hexdigest()}
    result={'original':str(args.image),'original_sha256':hashlib.sha256(args.image.read_bytes()).hexdigest(),
      'scale':scale,'method':'independent original PNG decode; actual long top/bottom textbox border runs in visually identified regions',
      'fields':records,'shared_retained_cpu_geometry_test':'separate root-owned evidence',
      'source_observer':False}
    (args.output/'field-geometry.json').write_text(json.dumps(result,indent=2)+'\n')
    print(json.dumps(result,indent=2))

if __name__=='__main__':
    main()
