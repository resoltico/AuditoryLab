from pathlib import Path
from html import escape
from math import sin, pi
import json

OUT = Path(__file__).parent
INK = '#18333d'
SUB = '#50616a'
LINE = '#d7e0e3'
ACCENT = '#147984'
PALE = '#eef6f6'
PAPER = '#ffffff'
BANDS = [('Delta', '0.1–<4 Hz', 2), ('Theta', '4–<8 Hz', 6), ('Alpha', '8–13 Hz', 10), ('Beta', '>13–30 Hz', 20), ('Gamma', '>30–80 Hz', 40)]

class Graphic:
    def __init__(self, title, height, description):
        self.height = height
        self.items = [f'<svg xmlns="http://www.w3.org/2000/svg" width="1120" height="{height}" viewBox="0 0 1120 {height}" role="img" aria-labelledby="title desc">',
            f'<title id="title">{escape(title)}</title><desc id="desc">{escape(description)}</desc>',
            f'<rect width="1120" height="{height}" fill="{PAPER}"/>',
            '<g font-family="Arial, Helvetica, sans-serif">']
    def text(self, x, y, content, size=20, color=INK, weight=400, anchor='start'):
        self.items.append(f'<text x="{x}" y="{y}" font-size="{size}" font-weight="{weight}" fill="{color}" text-anchor="{anchor}">{escape(content)}</text>')
    def lines(self, x, y, lines, size=20, color=SUB, gap=27, anchor='start', weight=400):
        for i, line in enumerate(lines): self.text(x, y+i*gap, line, size, color, weight, anchor)
    def line(self, x1, y1, x2, y2, color=LINE, width=1):
        self.items.append(f'<path d="M{x1} {y1} L{x2} {y2}" fill="none" stroke="{color}" stroke-width="{width}"/>')
    def rect(self, x, y, w, h, fill=PALE, stroke='none', radius=10):
        self.items.append(f'<rect x="{x}" y="{y}" width="{w}" height="{h}" rx="{radius}" fill="{fill}" stroke="{stroke}"/>')
    def circle(self, x, y, r, fill=ACCENT):
        self.items.append(f'<circle cx="{x}" cy="{y}" r="{r}" fill="{fill}"/>')
    def finish(self, name):
        self.items.append('</g></svg>')
        p = OUT / name
        p.write_text('\n'.join(self.items)+'\n')
        return p
    def footer(self, y, extra):
        self.line(48,y-23,1072,y-23)
        self.text(48,y,'Observed brain-rhythm associations; no listening effect is implied.',19,INK)
        self.text(48,y+29,extra,18,SUB)

# A: One unified frequency-and-context reference, with mathematically original traces.
a = Graphic('Five rhythm bands: rhythms and example contexts', 735,
    'Five equal-amplitude illustrative sine waves over one second at 2, 6, 10, 20 and 40 hertz. Text gives selected EEG contexts, not a sleep-to-alertness ranking or effects of auditory stimulation.')
a.text(48,52,'Five rhythm bands, with example contexts',32,INK,500)
a.text(48,86,'Frequency describes a signal. Its meaning also depends on location, timing and task.',20,SUB)
a.text(48,133,'BAND',18,SUB)
a.text(295,133,'SCHEMATIC · 1 SECOND',18,SUB)
a.text(625,133,'EXAMPLE CONTEXT',18,SUB)
contexts = [
    ('Deep NREM sleep',['Slow-wave activity is prominent.']),
    ('Drowsiness',['Theta also appears in waking memory tasks.']),
    ('Quiet wakefulness',['Posterior rhythm, especially with eyes closed.']),
    ('Wakefulness',['Beta is often recorded frontocentrally.']),
    ('Engaged neural networks',['Gamma is studied in local processing.'])
]
for i, ((name, interval, f), (head, detail)) in enumerate(zip(BANDS, contexts)):
    y=188+i*91
    a.text(48,y,name,26,INK,500)
    a.text(48,y+30,interval,20,SUB)
    center=y+7
    a.line(295,center,580,center)
    coords=[]
    for j in range(2001):
        t=j/2000
        coords.append(f'{"M" if j==0 else "L"}{295+285*t:.3f} {center-19*sin(2*pi*f*t):.3f}')
    a.items.append(f'<path d="{" ".join(coords)}" fill="none" stroke="{ACCENT}" stroke-width="2"/>')
    a.text(625,y,head,23,INK,500)
    a.lines(625,y+30,detail,19)
    if i<4: a.line(48,y+57,1072,y+57)
a.text(295,624,'0',18,SUB)
a.text(580,624,'1 s',18,SUB,anchor='end')
a.footer(671,'Schematic waves, not recordings. App band boundaries shown; conventions vary.')
a.finish('a-frequency-and-context.svg')

# B: Qualitative, non-exhaustive matrix. A mark is a documented example, not a score.
b = Graphic('EEG contexts overlap across frequency bands', 735,
    'A qualitative matrix links deep NREM sleep with delta, drowsiness with theta, eyes-closed quiet wakefulness with alpha, waking frontocentral EEG with beta, and a human memory task example with both theta and gamma. Blank cells do not assert absence.')
b.text(48,52,'A band can occur in more than one context',32,INK,500)
b.text(48,86,'Selected EEG / MEG observations—not a scale from sleepy to alert.',20,SUB)
centers=[480,612,744,876,1008]
for (name, interval, _), x in zip(BANDS,centers):
    b.text(x,146,name,24,INK,500,anchor='middle')
    b.text(x,177,interval,18,SUB,anchor='middle')
b.text(48,177,'CONTEXT',18,SUB)
rows=[
    ('Deep NREM sleep', 'Slow-wave sleep', [0]),
    ('Drowsiness', 'Transition toward sleep', [1]),
    ('Quiet wakefulness', 'Eyes closed; posterior alpha', [2]),
    ('Waking frontocentral EEG', 'A characteristic beta context', [3]),
    ('Memory task example', 'Human MEG research', [1,4]),
]
for i,(title,subtitle,hits) in enumerate(rows):
    y=233+i*73
    b.text(48,y,title,22,INK,500)
    b.text(48,y+28,subtitle,18,SUB)
    for j,x in enumerate(centers):
        if j in hits:
            b.circle(x,y+4,11)
        else:
            b.text(x,y+10,'—',20,LINE,anchor='middle')
    b.line(48,y+48,1072,y+48)
b.circle(59,623,8)
b.text(80,630,'Selected observed association',19,INK)
b.text(48,665,'Blank cells do not mean absence. Marks show neither strength nor likelihood.',18,SUB)
b.text(48,704,'These associations do not predict what listening to a rhythm will do.',18,SUB)
b.finish('b-overlapping-contexts.svg')

# C: Context-first grouping; repeated theta deliberately counters one-band/one-state teaching.
c = Graphic('Sleep and waking contexts with EEG examples', 695,
    'Three contextual groups show sleep-related examples, quiet wakefulness, and selected waking activity. Theta appears in both drowsiness and a waking memory-task example. These groups are not exclusive sets and do not imply causal effects of sound.')
c.text(48,52,'Start with the context, then name the rhythms',32,INK,500)
c.text(48,86,'Sleep, quiet wakefulness and task activity are not one frequency ladder.',20,SUB)
xs=[48,398,748]
for x in xs: c.rect(x,123,324,458,fill='#f3f7f8',stroke=LINE)
c.text(xs[0]+22,166,'Sleep-related examples',24,INK,500)
c.text(xs[1]+22,166,'Quiet wakefulness',24,INK,500)
c.text(xs[2]+22,166,'Waking examples',24,INK,500)

def label(g,x,y,name,interval):
    g.rect(x,y-28,280,67,fill=PAPER,stroke=LINE,radius=7)
    g.text(x+15,y,name,25,INK,500)
    g.text(x+15,y+27,interval,19,SUB)

c.text(xs[0]+22,215,'Deep NREM sleep',21,INK,500)
label(c,xs[0]+22,263,'Delta','0.1–<4 Hz')
c.line(xs[0]+22,326,xs[0]+302,326)
c.text(xs[0]+22,365,'Drowsiness',21,INK,500)
label(c,xs[0]+22,414,'Theta','4–<8 Hz')
c.lines(xs[0]+22,504,['Characteristic examples','from sleep recordings.'],18)

c.lines(xs[1]+22,215,['Eyes closed, relaxed'],21,INK,weight=500)
label(c,xs[1]+22,263,'Alpha','8–13 Hz')
c.lines(xs[1]+22,355,['Posterior alpha often weakens','with eye opening or attention.'],19)
c.lines(xs[1]+22,504,['Quiet wakefulness is still','an awake condition.'],18)

c.text(xs[2]+22,215,'Frontocentral activity',21,INK,500)
label(c,xs[2]+22,263,'Beta','>13–30 Hz')
c.line(xs[2]+22,326,xs[2]+302,326)
c.text(xs[2]+22,365,'Human MEG memory study',20,INK,500)
c.rect(xs[2]+22,385,280,86,fill=PAPER,stroke=LINE,radius=7)
c.text(xs[2]+37,419,'Theta + Gamma',25,INK,500)
c.text(xs[2]+37,450,'4–<8 Hz   ·   >30–80 Hz',18,SUB)
c.lines(xs[2]+22,504,['Two bands observed','during the same task.'],18)
c.footer(630,'Selected, non-exclusive examples. App bands shown; study boundaries can differ.')
c.finish('c-context-first.svg')

metadata={
 'prepared_date':'2026-10-30',
 'purpose':'Original standalone concepts for user selection; not inserted into onboarding UI',
 'original_artwork':'All compositions, paths and waveform geometry authored from scratch; no stock images or copied figures. Standard installed sans-serif glyph rendering; no fonts redistributed.',
 'scientific_scope':'Selected observed electrophysiological associations. Not a diagnostic map, quantitative power table, alertness scale, sound-to-brain transfer function, or predicted effect of auditory stimulation.',
 'limits':['Gamma is not a highest-alertness state.','Theta can occur in waking memory tasks.','Quiet eyes-closed alpha is wakefulness.','Examples are non-exhaustive and regional/task dependent.','Five app band boundaries are a stated convention, not universal cutoffs.'],
 'claim_sources':[
  {'claim':'Band naming; characteristic beta and gamma contexts; drowsiness examples; deep NREM/alpha context','source':'IFCN glossary, 2017, with app beta convention following its documented correction','url':'https://pmc.ncbi.nlm.nih.gov/articles/PMC6123891/'},
  {'claim':'Posterior alpha during eyes-closed relaxed wakefulness','source':'NCBI MeSH Alpha Rhythm','url':'https://www.ncbi.nlm.nih.gov/mesh/D000513'},
  {'claim':'Theta and gamma during a particular healthy-human declarative-memory task (MEG)','source':'Osipova et al., 2006, Theta and Gamma Oscillations Predict Encoding and Retrieval of Declarative Memory','url':'https://pmc.ncbi.nlm.nih.gov/articles/PMC6674196/','scope':'Original study ranges 4.5–8.5 and 60–90 Hz; diagrams use the app labels and disclose differing conventions.'},
  {'claim':'Sleep phases are not a monotonic low-to-high frequency or alertness ladder; REM can resemble waking activity','source':'NIH NHLBI, Sleep Phases and Stages','url':'https://www.nhlbi.nih.gov/health/sleep/stages-of-sleep'}],
 'copyright_note':'Original creation and absence of copied artwork do not guarantee copyrightability or exclusivity. Human creative contribution and applicable law govern protection. No ownership over scientific facts is asserted.',
 'legal_context':'https://intellectual-property-helpdesk.ec.europa.eu/regional-ip-helpdesks/european-ip-helpdesk/europe-frequently-asked-questions_en'
}
(OUT/'provenance.json').write_text(json.dumps(metadata,indent=2)+'\n')
print('Created 3 original SVG concepts and provenance.json')
