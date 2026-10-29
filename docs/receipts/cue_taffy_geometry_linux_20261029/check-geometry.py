from pathlib import Path
import re,json

def correlate(root,reverse_windows=False):
 data=json.loads((root/'run.json').read_text());xdg={};toplevels=[];active={};rects=[]
 for line in (root/'stderr.log').read_text().splitlines():
  if m:=re.search(r'get_xdg_surface\(new id xdg_surface[#@](\d+), wl_surface[#@](\d+)\)',line):xdg[m[1]]=m[2]
  if m:=re.search(r'xdg_surface[#@](\d+)\.get_toplevel',line):toplevels.append(xdg[m[1]])
  if m:=re.search(r'zwp_text_input_v3[#@](\d+)\.enter\(wl_surface[#@](\d+)\)',line):active[m[1]]=m[2]
  if m:=re.search(r'zwp_text_input_v3[#@](\d+)\.leave\(',line):active.pop(m[1],None)
  if m:=re.search(r'zwp_text_input_v3[#@](\d+)\.set_cursor_rectangle\((-?\d+), (-?\d+), (-?\d+), (-?\d+)\)',line):rects.append({'surface':active.get(m[1]),'rectangle':list(map(int,m.groups()[1:]))})
 assert len(toplevels)==2,toplevels
 if reverse_windows:toplevels=toplevels[::-1]
 scales={int(x['window']):float(x['native_window_scale']) for x in data['source_captures']};results={}
 for window in [0,1]:
  scale=scales[window];areas={tuple(a[k]/scale for k in ['x','y','width','height']) for a in data['shared_caret_observations'] if int(a.get('window',-1))==window};matches=[];missing=[]
  for a in sorted(areas):
   found=next((x for x in rects if x['surface']==toplevels[window] and all(abs(a[i]-x['rectangle'][i])<=1.0 for i in range(4))),None)
   if found:matches.append({'shared_logical_rectangle':list(a),'wayland_rectangle':found['rectangle']})
   else:missing.append(list(a))
  results[str(window)]={'wl_surface':toplevels[window],'native_scale':scale,'distinct_shared_rectangles':len(areas),'matched_rectangles':matches,'unmatched_rectangles':missing,'all_rectangles_match':bool(areas) and not missing}
 heights={w:sorted({round(a['height']/scales[w],4) for a in data['shared_caret_observations'] if int(a.get('window',-1))==w}) for w in [0,1]};zoom_pair=next(((a,b) for a in heights[0] for b in heights[0] if abs(b/a-1.1)<=0.01),None);lens_keeps_zoom=bool(zoom_pair) and all(abs(h-zoom_pair[1])<=1.0 for h in heights[1])
 return {'method':'Map protocol toplevel creation order to scenario window ids 0/1, then bind each text-input request to its most recent enter surface. Compare every distinct shared rectangle for that window within one logical pixel.','windows':results,'observed_logical_caret_heights':{str(k):v for k,v in heights.items()},'primary_zoom_pair':zoom_pair,'lens_keeps_zoom':lens_keeps_zoom,'qualified':data['qualified'] and all(v['all_rectangles_match'] for v in results.values()) and lens_keeps_zoom}

def raster_check(root):
 from PIL import Image
 rows=[]
 for path in sorted((root/'receiving').glob('*.sources.json')):
  for item in json.loads(path.read_text()):
   image_path=root/'receiving'/item['file']
   with Image.open(image_path) as image:
    image.load();extrema=image.convert('RGB').getextrema();nonblank=any(a!=b for a,b in extrema);size=list(image.size)
   rows.append({'capture':path.name,'file':item['file'],'window':item['window'],'size':size,'nonblank':nonblank,'adapter':item['adapter']})
 return {'captures':rows,'qualified':bool(rows) and all(r['nonblank'] and r['adapter']['backend']=='Vulkan' and r['adapter']['device_type'] not in ['Cpu','Other'] for r in rows) and {0,1}<={r['window'] for r in rows}}

if __name__=='__main__':
 import sys
 root=Path(sys.argv[1]);r=correlate(root);raster=raster_check(root);r['rasters']=raster;r['qualified'] &= raster['qualified'];(root/'geometry-validation.json').write_text(json.dumps(r,indent=2)+'\n');print(json.dumps(r,indent=2));raise SystemExit(0 if r['qualified'] else 1)
