from pathlib import Path
from math import sin, pi, cos
from html import escape
import json

OUT=Path(__file__).parent
W,H=1400,720
# Original geometric stroke lettering, drawn directly here; no font data is read.
GLYPHS={
'A':'M0 10L3.5 0L7 10M1.4 6H5.6',
'B':'M0 10V0H3.8Q7 0 7 2.5Q7 5 3.8 5H0M3.8 5Q7 5 7 7.5Q7 10 3.8 10H0',
'C':'M7 1.2Q5.9 0 3.5 0Q0 0 0 3.5V6.5Q0 10 3.5 10Q5.9 10 7 8.8',
'D':'M0 0V10H3Q7 10 7 5Q7 0 3 0Z',
'E':'M7 0H0V10H7M0 5H5.5',
'F':'M7 0H0V10M0 5H5.5',
'G':'M7 1.4Q5.8 0 3.5 0Q0 0 0 3.5V6.5Q0 10 3.5 10Q5.8 10 7 8.7V5.5H4.2',
'H':'M0 0V10M7 0V10M0 5H7',
'I':'M0 0H7M3.5 0V10M0 10H7',
'J':'M0 0H7V7Q7 10 3.5 10Q0 10 0 7',
'K':'M0 0V10M7 0L0 6M2.9 3.5L7 10',
'L':'M0 0V10H7',
'M':'M0 10V0L3.5 5.7L7 0V10',
'N':'M0 10V0L7 10V0',
'O':'M3.5 0Q0 0 0 3.5V6.5Q0 10 3.5 10Q7 10 7 6.5V3.5Q7 0 3.5 0Z',
'P':'M0 10V0H3.7Q7 0 7 2.6Q7 5.2 3.7 5.2H0',
'Q':'M3.5 0Q0 0 0 3.5V6.5Q0 10 3.5 10Q7 10 7 6.5V3.5Q7 0 3.5 0M4.5 7L8 11',
'R':'M0 10V0H3.7Q7 0 7 2.6Q7 5.2 3.7 5.2H0M3.5 5.2L7 10',
'S':'M7 1Q5.8 0 3.5 0Q0 0 0 2.5Q0 4.3 3.5 5Q7 5.7 7 7.5Q7 10 3.5 10Q1.2 10 0 9',
'T':'M0 0H7M3.5 0V10',
'U':'M0 0V6.5Q0 10 3.5 10Q7 10 7 6.5V0',
'V':'M0 0L3.5 10L7 0',
'W':'M0 0L1.5 10L3.5 4.5L5.5 10L7 0',
'X':'M0 0L7 10M7 0L0 10',
'Y':'M0 0L3.5 5L7 0M3.5 5V10',
'Z':'M0 0H7L0 10H7',
'0':'M3.5 0Q0 0 0 3.5V6.5Q0 10 3.5 10Q7 10 7 6.5V3.5Q7 0 3.5 0ZM1.4 8.5L5.6 1.5',
'1':'M1 2L3.7 0V10M0.8 10H6.8',
'2':'M0 2.4Q0 0 3.5 0Q7 0 7 2.6Q7 4.3 4.7 6L0 10H7',
'3':'M0 1Q1.1 0 3.5 0Q7 0 7 2.5Q7 5 3.5 5M2.7 5Q7 5 7 7.5Q7 10 3.5 10Q1.1 10 0 9',
'4':'M5.5 10V0L0 6.7H7',
'5':'M7 0H0V4.8H3.5Q7 4.8 7 7.4Q7 10 3.5 10Q1.1 10 0 9',
'6':'M6.7 1Q5.8 0 3.5 0Q0 0 0 4V6.8Q0 10 3.5 10Q7 10 7 7Q7 4.5 3.5 4.5H0',
'7':'M0 0H7L2 10',
'8':'M3.5 0Q0 0 0 2.5Q0 5 3.5 5Q7 5 7 2.5Q7 0 3.5 0ZM3.5 5Q0 5 0 7.5Q0 10 3.5 10Q7 10 7 7.5Q7 5 3.5 5Z',
'9':'M7 5.5H3.5Q0 5.5 0 2.8Q0 0 3.5 0Q7 0 7 3.3V6Q7 10 3.5 10Q1.2 10 0.3 9',
'-':'M1 5H6',
'.':'M3.4 9.5L3.6 9.5',
',':'M3.8 9L2.8 11',
':':'M3.4 2.3L3.6 2.3M3.4 7.7L3.6 7.7',
'/':'M0 10L7 0',
'<':'M6.5 1L0.5 5L6.5 9',
'>':'M0.5 1L6.5 5L0.5 9',
'(':'M5.5 0Q1.5 5 5.5 10',
')':'M1.5 0Q5.5 5 1.5 10',
'+':'M0 5H7M3.5 1.5V8.5'
}
DARK={'bg':'#081622','center':'#122f3a','ink':'#edf5f7','sub':'#aec2ce','line':'#294655','colors':['#74acf0','#66d3b8','#c5db73','#f2ad6b','#cc96e9']}
LIGHT={'bg':'#f8fafc','center':'#e9f1f5','ink':'#18344b','sub':'#466477','line':'#d0dde6','colors':['#275f9f','#096e5d','#667817','#9b5118','#7944aa']}
BANDS=[
 ('DELTA','0.1-<4 HZ',2,['DEEP NREM SLEEP'],['SLOW-WAVE','ACTIVITY']),
 ('THETA','4-<8 HZ',6,['DROWSINESS'],['ALSO WAKING','MEMORY TASKS']),
 ('ALPHA','8-13 HZ',10,['QUIET','WAKEFULNESS'],['ESPECIALLY','EYES CLOSED']),
 ('BETA','>13-30 HZ',20,['WAKING ACTIVITY'],['FRONT-CENTRAL','EEG ACTIVITY']),
 ('GAMMA','>30-80 HZ',32,['LOCAL','PROCESSING'],['ENGAGED NEURAL','NETWORKS'])
]

class Drawing:
 def __init__(self, style, theme):
  self.style=style; self.theme=theme; self.p=DARK if theme=='dark' else LIGHT; self.bits=[];self.boxes=[];self.column=None
  p=self.p
  self.bits=[f'<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="{W}" height="{H}" viewBox="0 0 {W} {H}" role="img" aria-labelledby="title desc">',
   f'<title id="title">{escape(style)}: five rhythm bands, {theme} appearance</title>',
   '<desc id="desc">Original vector illustration. Delta, Theta, Alpha, Beta and Gamma with the app frequency intervals and selected overlapping EEG associations. Normalized sine waves are schematic, not EEG recordings or predicted effects of listening. All lettering is original geometric paths; no external images, icons or fonts.</desc>', '<defs>']
  for ch,d in GLYPHS.items(): self.bits.append(f'<path id="g{ord(ch)}" d="{d}"/>')
  self.bits.append(f'<radialGradient id="field"><stop offset="0" stop-color="{p["center"]}"/><stop offset="1" stop-color="{p["bg"]}"/></radialGradient>')
  self.bits.append('<linearGradient id="spectrum" x1="0" x2="1" y1="0" y2="0">')
  for i,color in enumerate(p['colors']):self.bits.append(f'<stop offset="{i/4}" stop-color="{color}"/>')
  self.bits.append('</linearGradient>')
  for i,color in enumerate(p['colors']):
   self.bits.append(f'<radialGradient id="halo{i}"><stop offset="0" stop-color="{color}" stop-opacity="{.22 if theme=="dark" else .11}"/><stop offset="1" stop-color="{color}" stop-opacity="0"/></radialGradient>')
  self.bits.append('<filter id="glow" x="-25%" y="-100%" width="150%" height="300%"><feGaussianBlur stdDeviation="5"/></filter></defs>')
  self.rect(0,0,W,H,'url(#field)',0)
 def path(self,d,color,stroke=2,opacity=1,fill='none',extra=''):
  self.bits.append(f'<path d="{d}" fill="{fill}" stroke="{color}" stroke-width="{stroke}" stroke-linecap="round" stroke-linejoin="round" opacity="{opacity}" {extra}/>')
 def rect(self,x,y,w,h,fill,rx=0,stroke='none'):
  self.bits.append(f'<rect x="{x}" y="{y}" width="{w}" height="{h}" rx="{rx}" fill="{fill}" stroke="{stroke}"/>')
 def circle(self,cx,cy,r,fill,stroke='none',sw=1,opacity=1):
  self.bits.append(f'<circle cx="{cx}" cy="{cy}" r="{r}" fill="{fill}" stroke="{stroke}" stroke-width="{sw}" opacity="{opacity}"/>')
 def label(self,content,x,y,size=22,color=None,align='middle',weight=.72):
  # y is the top of glyph geometry; the known original metrics make bounds exact.
  text=content.upper(); advance=9; scale=size/10
  units=sum(5 if c==' ' else advance for c in text)-2
  width=units*scale
  left=x-width/2 if align=='middle' else x-width if align=='end' else x
  assert left>=-1 and left+width<=W+1,(text,left,width)
  if self.column is not None: assert width<=235,(self.style,text,width)
  self.boxes.append({'text':text,'x':round(left,3),'y':y,'width':round(width,3),'height':size*1.1})
  self.bits.append(f'<g data-lettering="{escape(text,quote=True)}" aria-label="{escape(text,quote=True)}" transform="translate({left:.3f} {y:.3f}) scale({scale:.5f})" fill="none" stroke="{color or self.p["ink"]}" stroke-width="{weight}" stroke-linecap="round" stroke-linejoin="round">')
  cursor=0
  for c in text:
   if c==' ': cursor+=5;continue
   if c not in GLYPHS: raise ValueError(c)
   self.bits.append(f'<use xlink:href="#g{ord(c)}" x="{cursor}"/>');cursor+=advance
  self.bits.append('</g>')
 def wave(self,x0,x1,cy,amplitude,cycles,color,glow=True,fill=False):
  count=max(80,int(cycles*32))
  pts=[(x0+(x1-x0)*i/count,cy-amplitude*sin(2*pi*cycles*i/count)) for i in range(count+1)]
  d=' '.join(f'{"M" if i==0 else "L"}{x:.2f} {y:.2f}' for i,(x,y) in enumerate(pts))
  if fill:
   self.bits.append(f'<path d="{d} L{x1} {cy} L{x0} {cy}Z" fill="{color}" opacity="{.14 if self.theme=="dark" else .09}"/>')
  if glow:self.path(d,color,9,.17,extra='filter="url(#glow)"')
  self.path(d,color,2.6,1)
 def ribbon(self,y,thick=28):
  self.path(f'M68 {y+thick/2} Q54 {y+thick/2} 54 {y} Q54 {y-thick/2} 68 {y-thick/2} H1310 V{y-thick} L1354 {y} L1310 {y+thick} V{y+thick/2}Z','none',0,1,'url(#spectrum)')
 def finish(self):
  self.bits.append('</svg>')
  name=self.style.lower().replace(' ','-')+'-'+self.theme
  out=OUT/(name+'.svg');out.write_text('\n'.join(self.bits)+'\n')
  for b in self.boxes:assert b['y']>=0 and b['y']+b['height']<H,(name,b)
  return {'file':out.name,'theme':self.theme,'lettering_boxes':self.boxes}

centers=[170,435,700,965,1230]

def spectrum(theme):
 d=Drawing('Luminous spectrum',theme);p=d.p
 d.label('FIVE RHYTHM BANDS',700,35,29,p['ink'],weight=.8)
 d.label('SELECTED EEG ASSOCIATIONS',700,83,20,p['sub'])
 for i,((name,hz,f,context,detail),cx,color) in enumerate(zip(BANDS,centers,p['colors'])):
  d.column=cx
  d.rect(cx-132.5,120,265,430,f'url(#halo{i})',0)
  if i<4:d.path(f'M{cx+132.5} 132V544',p['line'],1)
  d.label(name,cx,140,38,color,weight=1)
  d.label(hz,cx,197,22,p['ink'])
  for j,line in enumerate(context):d.label(line,cx,240+(2-len(context))*13+j*26,18,p['sub'])
  d.path(f'M{cx-132.5} 359H{cx+132.5}',p['line'],1.1)
  d.wave(cx-132.5,cx+132.5,359,61,f,color)
  for j,line in enumerate(detail):d.label(line,cx,466+j*28,18,color)
 d.column=None
 d.label('SCHEMATIC WAVES - ONE SECOND PER SECTION',700,553,20,p['sub'])
 d.ribbon(612,22)
 d.label('SLOWER',60,650,20,p['sub'],align='start')
 d.label('INCREASING FREQUENCY',700,650,22,p['ink'])
 d.label('FASTER',1348,650,20,p['sub'],align='end')
 d.label('BANDS OVERLAP. FREQUENCY IS NOT AN ALERTNESS SCALE.',700,692,16,p['sub'])
 return d.finish()

def ribbon(theme):
 d=Drawing('Spectral ribbon',theme);p=d.p
 d.label('A SPECTRUM OF RHYTHMS',700,38,31,p['ink'],weight=.9)
 d.label('FREQUENCY BANDS AND EXAMPLE CONTEXTS',700,87,20,p['sub'])
 for i,((name,hz,f,context,detail),cx,color) in enumerate(zip(BANDS,centers,p['colors'])):
  d.column=cx
  d.circle(cx,330,163,f'url(#halo{i})')
  d.label(name,cx,160,36,color,weight=1)
  d.label(hz,cx,217,23,p['ink'])
  d.wave(cx-132.5,cx+132.5,344,73,f,color,fill=True)
  d.circle(cx,440,3.5,color)
  d.path(f'M{cx} 420V451',color,1,.6)
  for j,line in enumerate(context):d.label(line,cx,467+(2-len(context))*13+j*26,18,p['ink'],weight=.8)
  for j,line in enumerate(detail):d.label(line,cx,540+j*26,17,p['sub'])
 d.column=None
 d.path('M54 344H1362',p['line'],1,.5)
 d.rect(55,599,1290,6,'url(#spectrum)',3)
 d.label('SLOWER RHYTHMS',56,630,20,p['sub'],align='start')
 d.label('FASTER RHYTHMS',1344,630,20,p['sub'],align='end')
 d.label('ONE SECOND PER SECTION - NORMALIZED SCHEMATIC WAVES',700,668,18,p['sub'])
 d.label('ASSOCIATIONS OVERLAP. NO LISTENING EFFECT IS IMPLIED.',700,700,15,p['sub'])
 return d.finish()

def medallions(theme):
 d=Drawing('Wave medallions',theme);p=d.p
 d.label('FIVE RHYTHM BANDS',700,35,30,p['ink'],weight=.9)
 d.label('A VISUAL GUIDE TO SELECTED EEG ASSOCIATIONS',700,83,19,p['sub'])
 for i,((name,hz,f,context,detail),cx,color) in enumerate(zip(BANDS,centers,p['colors'])):
  d.column=cx
  d.label(name,cx,144,36,color,weight=1)
  d.label(hz,cx,199,22,p['ink'])
  d.circle(cx,341,128,f'url(#halo{i})')
  d.circle(cx,341,92,'none',color,1.4,.55)
  d.circle(cx,341,83,'none',p['line'],1,.8)
  d.path(f'M{cx-76} 341H{cx+76}',p['line'],1)
  d.wave(cx-76,cx+76,341,35,f*.5,color)
  for j,line in enumerate(context):d.label(line,cx,455+(2-len(context))*13+j*26,18,p['ink'],weight=.8)
  for j,line in enumerate(detail):d.label(line,cx,525+j*26,17,p['sub'])
 d.column=None
 d.ribbon(605,14)
 d.label('INCREASING FREQUENCY - NOT A RANKING OF MENTAL STATES',700,645,18,p['ink'])
 d.label('HALF-SECOND SCHEMATICS. NOT EEG RECORDINGS OR PREDICTED AUDIO EFFECTS.',700,687,15,p['sub'])
 return d.finish()

results=[]
for draw in [spectrum,ribbon,medallions]:
 for theme in ['light','dark']:results.append(draw(theme))
(OUT/'geometry.json').write_text(json.dumps(results,indent=2)+'\n')
(OUT/'PROVENANCE.md').write_text('''# Original SVG studies

All graphic geometry, waveform paths, gradients, composition and geometric lettering were authored directly in draw_spectra.py for this request. No font files, stock artwork, icons, external images, traced silhouettes or public-domain assets are used. SVG lettering is made from original line/curve paths. The supplied Gemini image was a visual-direction reference only and is not included in the SVGs or used as pixel material.

Three compositions each have explicit light and dark SVG variants generated from identical layout logic. All text remains accessible through data-lettering/aria-label attributes and root descriptions; wording is edited in the generator. Asset pairs are appropriate for a native UI that selects its image on a palette change; no runtime download or SVG media-query support is required.

Content follows the existing app convention: Delta 0.1–<4, Theta 4–<8, Alpha 8–13 inclusive, Beta >13–30, Gamma >30–80 Hz. Those are app reference bands, not universal EEG cutoffs. Displayed contexts are selected observations, not a causal sound-to-state map or a ranking of intelligence/alertness. Sine waves are equal-amplitude schematic rhythms, with one second per section in the first two compositions and half a second per medallion in the third. No amplitude comparisons between actual EEG bands are implied.

Scientific sources already reviewed: IFCN glossary https://pmc.ncbi.nlm.nih.gov/articles/PMC6123891/ ; human waking-memory theta/gamma observations https://pmc.ncbi.nlm.nih.gov/articles/PMC6674196/ ; NIH sleep-stage overview https://www.nhlbi.nih.gov/health/sleep/stages-of-sleep . The strong wording in the reference concerning peak consciousness, positive thoughts and guaranteed restorative effects was not carried forward.

No onboarding implementation or repository source was changed. Original drawing and absence of borrowed assets are statements about the creation process; no new legal copyrightability guarantee is asserted.
''')
print(json.dumps({'svg_files':len(results),'total_svg_bytes':sum((OUT/r['file']).stat().st_size for r in results),'font_assets':0,'external_graphic_assets':0},indent=2))
