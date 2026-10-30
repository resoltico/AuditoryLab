from pathlib import Path
import base64
root=Path(__file__).parent
original=root/'auditorylab-start-here.html'
output=root/'auditorylab-onboarding-spectrum.html'
s=original.read_text().replace('al-onboarding-preview','al-onboarding-spectrum')
s=s.replace('<div id="al-onboarding-spectrum">', '''<div id="al-onboarding-spectrum">
  <div class="viz-controls">
    <label class="form-label" for="al-spectrum-appearance">Preview appearance</label>
    <select class="form-select" id="al-spectrum-appearance">
      <option value="system">Follow system</option>
      <option value="light">Light</option>
      <option value="dark">Dark</option>
    </select>
  </div>''',1)
css='''
#al-onboarding-spectrum .al-page.al-rhythm-page { max-width:none; }
#al-onboarding-spectrum .al-spectrum { margin:20px 0 12px; }
#al-onboarding-spectrum .al-spectrum img { display:block; width:100%; height:auto; border-radius:8px; }
#al-onboarding-spectrum .al-spectrum figcaption { margin-top:10px; font-size:12px; color:var(--al-sub); }
#al-onboarding-spectrum .al-spectrum-text { margin:10px 0 22px; }
#al-onboarding-spectrum .al-spectrum-text summary { color:var(--al-action); padding:5px 0; }
#al-onboarding-spectrum .al-spectrum-text dl { display:grid; grid-template-columns:115px 1fr; gap:8px 16px; margin:12px 0; }
#al-onboarding-spectrum .al-spectrum-text dt { font-weight:500; }
#al-onboarding-spectrum .al-spectrum-text dd { margin:0; color:var(--al-sub); }
#al-onboarding-spectrum .al-band-detail-heading { margin-top:22px; }
'''
s=s.replace('</style>',css+'</style>',1)
assets={theme:'data:image/svg+xml;base64,'+base64.b64encode((root/'rhythm-spectrum-studies'/('spectral-ribbon-'+theme+'.svg')).read_bytes()).decode() for theme in ['light','dark']}
import json
s=s.replace("  const saved = window.openai?.widgetState;", "  const spectrumAssets="+json.dumps(assets,separators=(',',':'))+";\n  const saved = window.openai?.widgetState;\n  const appearanceControl=root.querySelector('#al-spectrum-appearance');\n  const appearanceMedia=matchMedia('(prefers-color-scheme: dark)');\n  let appearance=['system','light','dark'].includes(saved?.privateContent?.appearance)?saved.privateContent.appearance:'system';\n  function resolvedAppearance(){return appearance==='system'?(appearanceMedia.matches?'dark':'light'):appearance;}\n  function applyAppearance(){const theme=resolvedAppearance();root.querySelectorAll('.al-app').forEach(app=>app.style.colorScheme=theme);appearanceControl.value=appearance;root.querySelectorAll('[data-spectrum-image]').forEach(image=>image.src=spectrumAssets[theme]);}\n")
s=s.replace("modelContent:{mockup:'Auditory Lab onboarding',screens:", "modelContent:{mockup:'Auditory Lab onboarding',selectedIllustration:'Spectral ribbon',appearance,screens:")
s=s.replace('privateContent:{revision:snapshotRevision,states:', 'privateContent:{revision:snapshotRevision,appearance,states:')
s=s.replace("(layout==='start'?'overview':'workbench')", "(layout==='start'?'bands':'workbench')")
start=s.index('    function rhythmBands()')
end=s.index('\n    function library()',start)
s=s[:start]+'''    function rhythmBands() {
      const b=bands[state.band];
      return `<div class="al-page al-rhythm-page">${back()}
        <h1>Five names for rhythm ranges</h1>
        <p class="al-intro">Delta, Theta, Alpha, Beta and Gamma are frequency-band names from EEG. Here, they group the rhythm rates in a sound’s definition.</p>
        <figure class="al-spectrum">
          <img data-spectrum-image src="${spectrumAssets[resolvedAppearance()]}" width="1400" height="720" alt="A spectrum of rhythms: five colored sections and a flowing wave. Delta: deep NREM sleep. Theta: drowsiness and waking memory tasks. Alpha: quiet eyes-closed wakefulness. Beta: waking, frontocentral activity. Gamma: local processing in engaged networks. These associations overlap and do not predict listening effects.">
          <figcaption>The overview shows selected EEG associations. The examples below show rhythms authored in a sound.</figcaption>
        </figure>
        <details class="al-spectrum-text"><summary>Read the diagram as text</summary><dl>
          <dt>Delta</dt><dd>0.1–&lt;4 Hz · Slow-wave activity is prominent in deep NREM sleep.</dd>
          <dt>Theta</dt><dd>4–&lt;8 Hz · Observed in drowsiness and also during waking memory tasks.</dd>
          <dt>Alpha</dt><dd>8–13 Hz · A posterior rhythm characteristic of quiet, eyes-closed wakefulness.</dd>
          <dt>Beta</dt><dd>&gt;13–30 Hz · Often recorded frontocentrally during wakefulness.</dd>
          <dt>Gamma</dt><dd>&gt;30–80 Hz · Studied in engaged or driven neural networks and local processing.</dd>
        </dl><p class="al-small">These are selected, overlapping observations. Band boundaries follow the app’s stated convention; study conventions vary.</p></details>
        <h2 class="al-band-detail-heading">Explore an individual rhythm</h2>
        <div class="al-band-picks" aria-label="Rhythm band examples">${bands.map((b,i)=>`<button type="button" data-band="${i}" aria-pressed="${state.band===i}">${b.name}<span>${b.range}</span></button>`).join('')}</div>
        <div class="al-figure"><div class="al-line-label"><strong>${b.name} example · ${b.rate} pulses / second</strong><span>One second</span></div><svg data-plot="band" role="img" aria-label="${b.rate} schematic pulses over one second"></svg></div>
        <p>The ${b.name} label describes a rhythm in this sound. It does not mean your brain is in a “${b.name} state.”</p>
        ${button('Explore pitch and rhythm','basics')}
      </div>`;
    }'''+s[end:]
s=s.replace("  window.addEventListener('openai:set_globals',event=>{", "  appearanceControl.addEventListener('change',()=>{appearance=appearanceControl.value;applyAppearance();remember();});\n  appearanceMedia.addEventListener('change',()=>{if(appearance==='system')applyAppearance();});\n  applyAppearance();\n  window.addEventListener('openai:set_globals',event=>{")
s=s.replace('    snapshotRevision=revision;\n    states.forEach', "    snapshotRevision=revision;\n    if(['system','light','dark'].includes(incoming.appearance))appearance=incoming.appearance;\n    applyAppearance();\n    states.forEach")
assert len(s.encode())<1000000
assert '<!doctype' not in s.lower()
output.write_text(s)
print('Updated mockup:',output)
print('Bytes:',output.stat().st_size)
