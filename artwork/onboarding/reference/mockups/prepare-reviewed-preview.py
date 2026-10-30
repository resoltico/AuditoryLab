from pathlib import Path
import re, json
ROOT=Path('/Users/erst/Tools/auditorylab')
VIS=Path('/Users/erst/.codex/visualizations/2026/10/29/01a18744-f3ff-7ee2-9369-2ea8d9f8e438')
PKG=ROOT/'target/handoffs/auditorylab-onboarding-artwork-2026-10-30'
script=PKG/'implementation-materials/artwork/onboarding/generate_art.py'
s=script.read_text().replace("        (output / name).write_text(svg, encoding='utf-8', newline='\\n')", "        with (output / name).open('w', encoding='utf-8', newline='\\n') as stream:\n            stream.write(svg)")
script.write_text(s)
mock=(VIS/'auditorylab-onboarding-spectrum.html').read_text()
examples=json.loads((PKG/'implementation-materials/crates/app/src/learning/band_examples.json').read_text())
palette=json.loads((PKG/'implementation-materials/artwork/onboarding/source/palette.json').read_text())
intervals={'delta':'0.1–<4 Hz','theta':'4–<8 Hz','alpha':'8–13 Hz','beta':'>13–30 Hz','gamma':'>30–80 Hz'}
bands=[{'name':entry['band'].title(),'range':intervals[entry['band']],'rate':entry['example_hz']} for entry in examples['examples']]
start=mock.index('  const bands = [');end=mock.index('\n  const types = [',start)
colors={theme:{key.title():data['COLOR_'+key.upper()] for key in intervals} for theme,data in palette['themes'].items()}
mock=mock[:start]+'  const bands = '+json.dumps(bands,ensure_ascii=False)+';\n  const illustrationColors='+json.dumps(colors)+';'+mock[end:]
mock=mock.replace('The pitch stays the same as you change how often it pulses.', 'The carrier remains at 440 Hz as you change how often it pulses.')
mock=mock.replace('${b.name} example · ${b.rate} pulses / second', 'Illustrative ${b.name} rhythm · ${b.rate} cycles / second')
mock=mock.replace('${b.rate} schematic pulses over one second', '${b.rate} cycles of a normalized sinusoidal illustration over one second; not EEG or audio')
old="else if(kind==='band')plot(svg,t=>envelope(t,bands[state.band].rate),1,`${bands[state.band].name} example, ${bands[state.band].rate} pulses per second`);"
new="else if(kind==='band'){const band=bands[state.band];plot(svg,t=>Math.sin(2*Math.PI*band.rate*t),1,`Illustrative ${band.name} rhythm, ${band.rate} cycles per second`);svg.style.setProperty('--al-wave',illustrationColors[resolvedAppearance()][band.name]);}"
assert old in mock
mock=mock.replace(old,new)
mock=mock.replace("root.querySelectorAll('[data-spectrum-image]').forEach(image=>image.src=spectrumAssets[theme]);}", "root.querySelectorAll('[data-spectrum-image]').forEach(image=>image.src=spectrumAssets[theme]);states.forEach(state=>state.draw?.());}")
mock=mock.replace('    state.restore=(next)=>{', '    state.draw=drawAll;\n    state.restore=(next)=>{')
mock=mock.replace("${button('New sound','create:pulsed')}", "${button('New sound','new-blank')}")
mock=mock.replace("state.draft=fresh(kind,record); if(rateOverride!==null)state.draft.rate=rateOverride;", "state.draft=kind===null?null:fresh(kind,record); if(rateOverride!==null&&state.draft)state.draft.rate=rateOverride;")
mock=mock.replace("else if(a==='edit-study')create", "else if(a==='new-blank')create(null);\n        else if(a==='edit-study')create")
mock=mock.replace("button('Browse built-in sounds','library')", "button('Browse Library','library')")
mock=mock.replace('The overview shows selected EEG associations. The examples below show rhythms authored in a sound.', 'The overview shows selected EEG associations. The individual diagrams below use the same example rates and one-second normalization.')
mock=mock.replace('The ${b.name} label describes a rhythm in this sound. It does not mean your brain is in a “${b.name} state.”', 'In the Library, a ${b.name} label classifies an authored rhythm rate. It does not establish the listener’s brain state.')
# This remains a mixed-content historical/design reference, not MPL production application source.
mixed='''<!-- Copyright (c) 2026 Ervins Strauhmanis.
SPDX-License-Identifier: MPL-2.0 AND LicenseRef-Proprietary-Artwork
Preview application code is MPL-covered. Embedded illustration payloads are
separately proprietary artwork; see package licensing and source materials.
This is a design reference, not production save/audio/application logic. -->
'''
reviewed=VIS/'auditorylab-onboarding-reviewed.html'
reviewed.write_text(mixed+mock)
(PKG/'reference/mockups/auditorylab-onboarding-reviewed.html').write_text(mixed+mock)
# Portable preview: explicit external artwork references keep its MPL code separate.
portable=mock
start=portable.index('  const spectrumAssets=');end=portable.index('\n  const saved = ',start)
assets={theme:'../implementation-materials/artwork/onboarding/assets/spectral-ribbon-'+theme+'.svg' for theme in ['light','dark']}
portable=portable[:start]+'  const spectrumAssets='+json.dumps(assets)+';'+portable[end:]
# Only the user-selected Start here approach is active in this portable reference.
portable=re.sub(r'\s*<section data-variant="Learn alongside".*?</section>', '', portable, count=1, flags=re.S)
portable=portable.replace('class="viz-carousel"','class="preview-stage"')
portable=portable.replace('<div class="viz-controls">','<div class="preview-controls">')
header='''<!-- Copyright (c) 2026 Ervins Strauhmanis.
SPDX-License-Identifier: MPL-2.0
This Source Code Form is subject to the terms of the Mozilla Public License,
v. 2.0. See https://mozilla.org/MPL/2.0/ . External artwork retains its separate
proprietary terms in implementation-materials/LICENSE. No artwork payload is
licensed by this preview's software notice. All save/audio behavior is mocked.
-->
'''
wrap='''<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>Auditory Lab onboarding design reference</title><style>body{font-family:system-ui,sans-serif;margin:16px;background:#fff;color:#20262c}.preview-controls{display:flex;gap:12px;align-items:center;margin-bottom:12px}.preview-controls select{font:inherit;padding:5px}.preview-stage{max-width:1100px;margin:auto}</style></head><body>'''
(PKG/'preview/index.html').write_text(header+wrap+portable+'</body></html>\n')
print(json.dumps({'corrected_mockup':str(reviewed),'portable_preview':str(PKG/'preview/index.html'),'representative_rates':[b['rate'] for b in bands]},indent=2))
