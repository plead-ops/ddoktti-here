"""Trace original poses with reviewed crops, one palette and measured proportions.
Run in the Python 3.13 venv described in docs/vector-artwork.md.
"""
import colorsys
import json
import re
import xml.etree.ElementTree as ET
from functools import lru_cache
from pathlib import Path
from PIL import Image, ImageFilter
import vtracer
import runpy
REPAIRS=runpy.run_path(str(Path(__file__).with_name("pet-art-repairs.py")))
ROOT=Path(__file__).resolve().parents[1]
SOURCE=ROOT/'assets/concepts/raster-before-svg'
OUT=ROOT/'apps/desktop/public/sprites/vector'
FRAMES=OUT/'frames'
FRAMES.mkdir(parents=True,exist_ok=True)
guides=json.loads((ROOT/'assets/concepts/pet-frame-guides.json').read_text())
PALETTE={'ink':'#081724','suit':'#26374F','green':'#00D66B','blue':'#075BD8','white':'#FFFEFA','red':'#F32643','mouth':'#8C1831','yellow':'#F4C637','orange':'#FF941C','cyan':'#24B5ED','pink':'#FF829C','sky':'#9ED4FF','cream':'#FFF0B8'}
# Bedding colours only exist on the sleep sheet; other frames never snap to them.
BED_KEYS=('sky','cream')
@lru_cache(maxsize=200000)
def colour(r,g,b,outline=True,bed=False):
    h,s,v=colorsys.rgb_to_hsv(r/255,g/255,b/255);h*=360
    ink_distance=sum((a-b)**2 for a,b in zip((r,g,b),(8,23,36)))
    suit_distance=sum((a-b)**2 for a,b in zip((r,g,b),(38,55,79)))
    if v<.145 or (outline and v<.4 and (172<=h<270 or s<.3) and ink_distance<suit_distance):key='ink'
    elif bed and 172<=h<240 and .24<=s<.6 and v>.7:key='sky'
    elif bed and 35<h<=75 and .24<=s<.5 and v>.85:key='cream'
    elif s<.24:key='white' if v>.62 else ('ink' if v<.22 else 'suit')
    elif 75<h<172:key='green' if v>.3 else 'ink'
    elif 172<=h<270:key='suit' if v<.4 else ('cyan' if h<204 else 'blue')
    elif 35<h<=75:key='yellow'
    elif 12<h<=35:key='orange'
    elif v<.58:key='mouth'
    else:key='pink' if s<.65 else 'red'
    return tuple(bytes.fromhex(PALETTE[key][1:]))

def clean_pixels(im,suit_from=None,keep_dark=False,bed=False):
    rgba=im.convert('RGBA');rgb=rgba.convert('RGB').filter(ImageFilter.MedianFilter(3))
    pixels=list(rgb.get_flattened_data());alpha=list(rgba.getchannel('A').get_flattened_data())
    colours=[colour(*c,bed=bed) for c in pixels]
    if suit_from is not None:
        # The source coat has dark AI shading close to the outline colour. Raising
        # the global black threshold makes camouflage-shaped holes in that coat.
        # Keep the original connected cloth regions flat, but apply the improved
        # ink classification to the face/glasses/antenna and external outlines.
        legacy=[colour(*c,False,bed) for c in pixels]
        suit=tuple(bytes.fromhex(PALETTE['suit'][1:]));width,height=im.size
        remaining=bytearray(int(c==suit and a>=128) for c,a in zip(legacy,alpha))
        minimum=max(24,round(width*height*.00015))
        for start in range(len(remaining)):
            if not remaining[start]:continue
            component=[start];remaining[start]=0;bottom=start//width
            for index in component:
                x,y=index%width,index//width;bottom=max(bottom,y)
                adjacent=[]
                if x:adjacent.append(index-1)
                if x+1<width:adjacent.append(index+1)
                if y:adjacent.append(index-width)
                if y+1<height:adjacent.append(index+width)
                for other in adjacent:
                    if remaining[other]:remaining[other]=0;component.append(other)
            # Shoes are drawn almost black (value ~0.15) while coat shading stays
            # lighter; a near-black region keeps the outline/shoe colour instead of
            # being flattened into the coat (chase sheet shoes).
            darkness=sum(max(pixels[index])/255 for index in component)/len(component) if keep_dark else 1
            if bottom>=suit_from and len(component)>=minimum and darkness>=.19:
                for index in component:colours[index]=suit
    clean=Image.new('RGBA',im.size)
    clean.putdata([(*c,255 if a>=128 else 0) for c,a in zip(colours,alpha)])
    return clean

def trace(im,suit_from=None,keep_dark=False,bed=False):
    clean=clean_pixels(im,suit_from,keep_dark,bed)
    svg=vtracer.convert_pixels_to_svg(list(clean.get_flattened_data()),clean.size,colormode='color',hierarchical='stacked',mode='spline',filter_speckle=4,color_precision=8,layer_difference=0,corner_threshold=60,length_threshold=4.0,max_iterations=10,splice_threshold=45,path_precision=2)
    # Tracer averages edge clusters: snap output fills to the exact shared colours.
    allowed=[v for k,v in PALETTE.items() if bed or k not in BED_KEYS]
    svg=re.sub(r'fill="#[0-9A-Fa-f]{6}"',lambda m:'fill="'+min(allowed,key=lambda c:sum((a-b)**2 for a,b in zip(bytes.fromhex(c[1:]),bytes.fromhex(m[0][7:13]))))+'"',svg)
    return ET.fromstring(svg)

def paths(root,transform=lambda x,y:(x,y)):
    result=[]
    for path in root:
        if not path.tag.endswith('path'):continue
        translate=re.search(r'translate\(([-\d.]+)[ ,]+([-\d.]+)\)',path.get('transform',''))
        tx,ty=map(float,translate.groups()) if translate else (0,0)
        tokens=re.findall(r'[A-Za-z]|-?\d*\.?\d+(?:e[-+]?\d+)?',path.get('d',''))
        out=[];axis=0;command=''
        for token in tokens:
            if token.isalpha():
                if token not in ['M','L','C','Q','Z']:raise ValueError('Unexpected trace command '+token)
                command=token;out.append(token);axis=0
            else:
                if axis%2==0:x=float(token)+tx
                else:
                    xx,yy=transform(x,float(token)+ty);out.extend([f'{xx:.2f}',f'{yy:.2f}'])
                axis+=1
        result.append('<path d="'+' '.join(out)+'" fill="'+path.get('fill')+'"/>')
    return ''.join(result)

def svg(body,width=400,height=260):
    return f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {width} {height}" width="{width}" height="{height}">{body}</svg>\n'

def generated_frames(atlas,manifest):
    directory=ROOT/'assets/concepts/generated'
    sources=json.loads((directory/'frame-guides.json').read_text())
    for source in sources:
        im=Image.open(directory/source['source']).convert('RGBA')
        for guide in source['frames']:
            sheet,index=guide['sheet'],guide['index']
            tile=im.crop((guide['x'],guide['y'],guide['x']+guide['width'],guide['y']+guide['height']))
            alpha=tile.getchannel('A').point(lambda a:255 if a>=128 else 0)
            bounds=alpha.getbbox()
            if not bounds:raise ValueError('Empty generated frame')
            left,top,right,bottom=bounds
            scale=90/guide['eyeSpan']
            def transform(x,y):return 200+(x-guide['headCenter'])*scale*(-1 if guide.get('mirror') else 1),250+(y-bottom)*scale
            key=f'{sheet}-{index}'
            atlas[key]=paths(trace(tile,suit_from=guide['neck'],keep_dark=True,bed=guide.get('palette')=='bed'),transform)
            hits=[]
            for y in range(top,bottom,16):
                end=min(y+16,bottom);box=alpha.crop((0,y,tile.width,end)).getbbox()
                if box:
                    x1,y1=transform(box[0],y);x2,y2=transform(box[2],end)
                    hits.append([round(min(x1,x2),3),round(y1,3),round(abs(x2-x1),3),round(y2-y1,3)])
            l,t=transform(left,top);rr,bb=transform(right,bottom);l,rr=min(l,rr),max(l,rr)
            frame=dict(source=guide,sourceImage='assets/concepts/generated/'+source['source'],width=400,height=260,left=l,top=t,right=rr,bottom=bb,hit=hits,url=f'/sprites/vector/frames/{key}.svg',key=key,eyeSpan=90,bodyHeight=round((bottom-guide['neck'])*scale,3),uniformScale=scale,sourceBounds=list(bounds),antennaRepaired=False)
            target=manifest.setdefault(sheet,dict(referenceHeight=170,frames=[]))['frames']
            if index<len(target):target[index]=frame
            elif index==len(target):target.append(frame)
            else:raise ValueError('Generated frame index gap')
        print(source['source']+': generated raster poses traced',flush=True)

def main():
    # Reuse the original antenna (including its outline and stem), behind raised arms.
    antenna=trace(Image.open(SOURCE/'companion/surfaces-v2.png').crop((100,9,149,69)))
    hand_guides=json.loads((ROOT/'assets/concepts/pet-climb-hand-guides.json').read_text())
    manifest={};atlas={};hands={}
    for name,records in guides.items():
        im=Image.open(SOURCE/'companion'/f'{name}.png').convert('RGBA');output=[]
        for index,guide in enumerate(records):
            r=dict(guide);tile=im.crop((r['x'],r['y'],r['x']+r['width'],r['y']+r['height']))
            alpha=tile.getchannel('A').point(lambda a:255 if a>=128 else 0)
            bounds=alpha.getbbox()
            if not bounds:raise ValueError(f'Empty frame: {name}/{index}')
            left,top,right,bottom=bounds
            scale=90/r['eyeSpan'];neck=r['neck']
            body=max(1,bottom-neck)
            # One uniform scale for the entire pose: never compress the torso or legs.
            def transform(x,y):
                yy=250+(y-bottom)*scale
                return 200+(x-r['headCenter'])*scale,yy
            if name=='surfaces-v2' and str(index) in hand_guides:
                hands[str(index)]=[[round(v,2) for v in transform(x,y)] for x,y in hand_guides[str(index)]]
            drawing=''
            if r.get('repairAntenna'):
                ant_x=r['headCenter']-r['eyeSpan']*.36
                ant_y=r['headTop']-r['eyeSpan']*.24
                ant_scale=r['eyeSpan']/125
                drawing=paths(antenna,lambda x,y:transform(ant_x+x*ant_scale,ant_y+y*ant_scale))
                left=min(left,ant_x);top=min(top,ant_y)
            drawing+=paths(trace(tile,suit_from=neck),transform)
            key=f'{name}-{index}'
            drawing=REPAIRS['repair'](key,drawing)
            (FRAMES/f'{key}.svg').write_text(svg(drawing))
            atlas[key]=drawing
            hits=[]
            for y in range(bounds[1],bounds[3],16):
                end=min(y+16,bounds[3]);box=alpha.crop((0,y,tile.width,end)).getbbox()
                if box:
                    x1,y1=transform(box[0],y);x2,y2=transform(box[2],end)
                    hits.append([round(x1,3),round(y1,3),round(x2-x1,3),round(y2-y1,3)])
            if r.get('repairAntenna'):
                x1,y1=transform(ant_x,ant_y);x2,y2=transform(ant_x+49*ant_scale,ant_y+60*ant_scale)
                hits.append([x1,y1,x2-x1,y2-y1])
            l,t=transform(left,top);rr,bb=transform(right,bottom)
            output.append(dict(source=guide,width=400,height=260,left=l,top=t,right=rr,bottom=bb,hit=hits,url=f'/sprites/vector/frames/{key}.svg',key=key,eyeSpan=90,bodyHeight=round(body*scale,3),uniformScale=scale,sourceBounds=[left,top,right,bottom],antennaRepaired=bool(r.get('repairAntenna'))))
        manifest[name]=dict(referenceHeight=170,frames=output)
        print(f'{name}: {len(output)} frames aligned and coloured',flush=True)
    generated_frames(atlas,manifest)
    for key,drawing in atlas.items():
        (FRAMES/f'{key}.svg').write_text(svg(drawing))
    (ROOT/'apps/desktop/src/pet-vector-frames.json').write_text(json.dumps(manifest,separators=(',',':'))+'\n')
    (ROOT/'apps/desktop/src/pet-vector-paths.json').write_text(json.dumps(atlas,separators=(',',':'))+'\n')
    (ROOT/'apps/desktop/src/pet-climb-hands.json').write_text(json.dumps(hands,separators=(',',':'))+'\n')
    (ROOT/'apps/desktop/src/pet-palette.json').write_text(json.dumps(PALETTE,indent=2)+'\n')
    welcome=manifest['emotions-v1']['frames'][6]
    view=f"{welcome['left']-8:.2f} {welcome['top']-8:.2f} {welcome['right']-welcome['left']+16:.2f} {welcome['bottom']-welcome['top']+16:.2f}"
    (OUT/'welcome.svg').write_text(svg(atlas['emotions-v1-6']).replace('viewBox="0 0 400 260"',f'viewBox="{view}"').replace('width="400" height="260"',''))
    icon=Image.open(SOURCE/'app-icon-1024.png')
    (ROOT/'assets/icons/app-icon.svg').write_text(svg(paths(trace(icon)),icon.width,icon.height))
    print('SVG atlas, palette, welcome and app icon generated.')

if __name__=='__main__':
    main()
