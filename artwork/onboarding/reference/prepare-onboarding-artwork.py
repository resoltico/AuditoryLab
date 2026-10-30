from pathlib import Path
import ast, re, json, shutil, hashlib
from math import sin, pi
ROOT=Path('/Users/erst/Tools/auditorylab')
VIS=Path('/Users/erst/.codex/visualizations/2026/10/29/01a18744-f3ff-7ee2-9369-2ea8d9f8e438')
PKG=ROOT/'target/handoffs/auditorylab-onboarding-artwork-2026-10-30'
ART=PKG/'implementation-materials/artwork/onboarding'
SRC=ART/'source'; ASSETS=ART/'assets'; REF=ART/'reference'
for p in [SRC,ASSETS,REF,PKG/'reference/mockups',PKG/'preview',PKG/'qa',PKG/'materials',PKG/'implementation-materials/crates/app/src/learning']:
 p.mkdir(parents=True,exist_ok=True)
raw=(VIS/'rhythm-spectrum-studies/draw_spectra.py').read_text()
module=ast.parse(raw)
values={}
for node in module.body:
 if isinstance(node,ast.Assign) and len(node.targets)==1 and isinstance(node.targets[0],ast.Name) and node.targets[0].id in ['GLYPHS','DARK','LIGHT','BANDS']:
  values[node.targets[0].id]=ast.literal_eval(node.value)
# Preserve every generated art direction and its original art-bearing source, without duplicate ZIP copies.
inventory=[]
for folder in ['rhythm-context-options','rhythm-spectrum-studies']:
 for f in sorted((VIS/folder).iterdir()):
  if f.is_file():
   dest=REF/folder/f.name;dest.parent.mkdir(parents=True,exist_ok=True);shutil.copy2(f,dest)
   inventory.append({'original':str(f),'package':str(dest.relative_to(PKG)),'sha256':hashlib.sha256(f.read_bytes()).hexdigest(),'role':'historical unselected/selected art source and prior exports; not runtime authority'})
for name in ['auditorylab-start-here.html','auditorylab-onboarding-spectrum.html','rhythm-spectrum-review.html','incorporate-spectrum.py']:
 f=VIS/name;dest=PKG/'reference/mockups'/name;shutil.copy2(f,dest)
 inventory.append({'original':str(f),'package':str(dest.relative_to(PKG)),'sha256':hashlib.sha256(f.read_bytes()).hexdigest(),'role':'mixed-content historical mockup or preview helper; not production source'})
# All factual representative rates have one implementation input. Native Band owns membership/ranges.
examples={'schema':1,'window_seconds':1,'normalized_amplitude':1,'initial_phase_cycles':0,'examples':[{'band':b[0].lower(),'example_hz':b[2]} for b in values['BANDS']]}
(PKG/'implementation-materials/crates/app/src/learning/band_examples.json').write_text(json.dumps(examples,indent=2)+'\n')
notice='''Copyright (c) 2026 Ervins Strauhmanis. All rights reserved.
SPDX-License-Identifier: LicenseRef-Proprietary-Artwork

The root Auditory Lab LICENSE proprietary artwork terms govern these drawing inputs and generated art. The MPL generator's license does not grant rights to modify or independently reuse this artwork. Existing icon permissions and previously granted rights are preserved. Scientific facts, mathematical definitions and legally required rights are not claimed exclusively.
'''
(SRC/'LICENSE').write_text(notice)
(SRC/'lettering.json').write_text(json.dumps({'copyright':'Copyright (c) 2026 Ervins Strauhmanis. All rights reserved.','license':'LicenseRef-Proprietary-Artwork','advance':9,'space_advance':5,'glyphs':values['GLYPHS']},indent=2)+'\n')
palette={'copyright':'Copyright (c) 2026 Ervins Strauhmanis. All rights reserved.','license':'LicenseRef-Proprietary-Artwork','themes':{}}
for mode,key in [('light','LIGHT'),('dark','DARK')]:
 v=values[key]
 p={'APPEARANCE':mode,'BACKGROUND':v['bg'],'FIELD':v['center'],'INK':v['ink'],'SUB':v['sub'],'LINE':v['line'],'HALO_OPACITY':'.22' if mode=='dark' else '.11','FILL_OPACITY':'.14' if mode=='dark' else '.09'}
 p.update({'COLOR_'+b[0]:color for b,color in zip(values['BANDS'],v['colors'])})
 palette['themes'][mode]=p
(SRC/'palette.json').write_text(json.dumps(palette,indent=2)+'\n')
art_comment='<!-- Copyright (c) 2026 Ervins Strauhmanis. All rights reserved.\nSPDX-License-Identifier: LicenseRef-Proprietary-Artwork\nSee the root Auditory Lab LICENSE for artwork permissions. -->\n'
# SVG masters own creative composition and glyph placements. Mathematical wave slots are regenerated.
dark=(VIS/'rhythm-spectrum-studies/spectral-ribbon-dark.svg').read_text()
dark=art_comment+dark
replacements={'#081622':'BACKGROUND','#122f3a':'FIELD','#edf5f7':'INK','#aec2ce':'SUB','#294655':'LINE'}
replacements.update({color:'COLOR_'+b[0] for b,color in zip(values['BANDS'],values['DARK']['colors'])})
for color,token in replacements.items():dark=dark.replace(color,'${'+token+'}')
dark=dark.replace('dark appearance','${APPEARANCE} appearance').replace('stop-opacity="0.22"','stop-opacity="${HALO_OPACITY}"').replace('opacity="0.14"','opacity="${FILL_OPACITY}"')
# Replace original glyph definitions with one shared proprietary lettering input.
pattern=r'<path id="g[0-9]+" d="[^"]*"/>'
glyph_matches=re.findall(pattern,dark)
assert len(glyph_matches)==len(values['GLYPHS'])
dark=re.sub(pattern,'',dark)
dark=dark.replace('<defs>','<defs>\n${GLYPH_DEFINITIONS}',1)
long_paths=[]
for d in re.findall(r'<path d="([^"]+)"',dark):
 if len(d)>1000 and not d.endswith('Z') and d not in long_paths:long_paths.append(d)
assert len(long_paths)==5
slots={}
centers=[170,435,700,965,1230]
for b,cx,path in zip(values['BANDS'],centers,long_paths):
 token='WAVE_'+b[0]
 assert dark.count(path)==3,(b[0],dark.count(path))
 dark=dark.replace(path,'${'+token+'}')
 slots[token]={'band':b[0].lower(),'x0':cx-132.5,'x1':cx+132.5,'center_y':344,'amplitude':73}
(SRC/'spectral-ribbon.svg.in').write_text(dark)
layout={'license':'LicenseRef-Proprietary-Artwork','masters':{'spectral-ribbon':{'template':'spectral-ribbon.svg.in','wave_slots':slots}}}
# Five matched illustrative rhythm masters: same phase, duration, representative rate, normalization and colors.
def glyph_label(text,x,y,size,color,align='start'):
 advance=9;scale=size/10;units=sum(5 if c==' ' else advance for c in text)-2
 width=units*scale;left=x if align=='start' else x-width if align=='end' else x-width/2
 parts=[f'<g data-lettering="{text}" aria-label="{text}" transform="translate({left:.3f} {y}) scale({scale:.5f})" fill="none" stroke="${{{color}}}" stroke-width=".8" stroke-linecap="round" stroke-linejoin="round">']
 cursor=0
 for char in text:
  if char==' ':cursor+=5;continue
  parts.append(f'<use xlink:href="#g{ord(char)}" x="{cursor}"/>');cursor+=9
 return '\n'.join(parts+['</g>'])
for b in values['BANDS']:
 name=b[0].lower();token='WAVE_'+b[0]
 pieces=[art_comment,'<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="1000" height="260" viewBox="0 0 1000 260" role="img" aria-labelledby="title desc">',f'<title id="title">{b[0].title()} illustrative rhythm, ${{EXAMPLE_HZ}} cycles per second, ${{APPEARANCE}} appearance</title>','<desc id="desc">Normalized one-second sinusoidal frequency illustration. Not recorded EEG, a pulse envelope or rendered audio. It uses the same representative rhythm as the combined spectrum.</desc>','<defs>${GLYPH_DEFINITIONS}<filter id="wave-glow" x="-20%" y="-100%" width="140%" height="300%"><feGaussianBlur stdDeviation="4"/></filter></defs>','<rect width="1000" height="260" rx="10" fill="${BACKGROUND}"/>',glyph_label(b[0]+' - ILLUSTRATIVE RHYTHM',55,22,22,'INK'),'${RATE_LABEL}', '<path d="M55 78V194 M55 136H947" stroke="${LINE}" fill="none"/>',f'<path d="${{{token}}}" stroke="${{COLOR_{b[0]}}}" stroke-width="8" opacity=".16" fill="none" filter="url(#wave-glow)"/>',f'<path data-wave="{name}" data-example-hz="${{EXAMPLE_HZ}}" d="${{{token}}}" stroke="${{COLOR_{b[0]}}}" stroke-width="2.6" stroke-linecap="round" stroke-linejoin="round" fill="none"/>',glyph_label('1',18,79,18,'SUB'),glyph_label('0',18,127,18,'SUB'),glyph_label('-1',10,179,18,'SUB'),glyph_label('0',55,209,18,'SUB'),glyph_label('1 SECOND',947,209,18,'SUB','end'),glyph_label('NORMALIZED SCHEMATIC - NOT AN EEG RECORDING',500,242,14,'SUB','middle'),'</svg>']
 (SRC/(name+'-rhythm.svg.in')).write_text('\n'.join(pieces)+'\n')
 layout['masters'][name+'-rhythm']={'template':name+'-rhythm.svg.in','band':name,'wave_slots':{token:{'band':name,'x0':55,'x1':947,'center_y':136,'amplitude':52}},'rate_label':{'x':947,'y':24,'size':20,'color':'SUB','align':'end'}}
(SRC/'layout.json').write_text(json.dumps(layout,indent=2)+'\n')
# Proposed license update is a reviewable deliverable, not a live repository edit.
license_text=(ROOT/'LICENSE').read_text()
old='and images generated from those files. These files and images are excluded from the MPL grant above.'
new='artwork/onboarding/source, artwork/onboarding/assets, artwork/onboarding/reference, and images generated from those files. These files and images are excluded from the MPL grant above.'
assert license_text.count(old)==1
license_text=license_text.replace(old,new)
(PKG/'implementation-materials/LICENSE').write_text(license_text)
(PKG/'materials/retained-art-inventory.json').write_text(json.dumps(inventory,indent=2)+'\n')
print(json.dumps({'staging':str(PKG),'historical_inputs_preserved':len(inventory),'selected_and_individual_masters':len(layout['masters'])},indent=2))
