//! Edit, compile, export and generate live audio with the library's default
//! features, and confirm the shared workspace refuses device playback.
use auditorylab::workspace::{Capabilities, Refusal, Workspace};
use auditorylab::workflow::{Cancel, Progress, Replace};
use auditorylab::editing::{Draft, EditingField, EditorSession};
use auditorylab::session::{LiveGenerator};
use auditorylab::core::{ParameterId, SampleRate};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let catalog = auditorylab::recipe::catalog()?;
    let snapshot = catalog.first().ok_or("empty catalog")?.clone();
    let rate = SampleRate::DEFAULT;

    // Editing: stage and commit through a field handle.
    let mut editor = EditorSession::new(Draft::from_snapshot(&snapshot)?)?;
    let component = *editor
        .draft()
        .components()
        .first()
        .ok_or("recipe without components")?;
    let parameter=match component.node {auditorylab::core::Node::Voice(_)=>ParameterId::PeakLevel,_=>ParameterId::PairFirstLevel};
    let level = editor.handle(EditingField::Parameter(component.id, parameter)).map_err(|refusal| format!("{refusal:?}"))?;
    editor
        .stage(level, "-40")
        .map_err(|refusal| format!("stage refused: {refusal:?}"))?;
    editor
        .commit()
        .map_err(|refusal| format!("commit refused: {refusal:?}"))?;
    editor
        .save_readiness()
        .map_err(|refusal| format!("not saveable: {refusal:?}"))?;

    // Compiling and rendering to a file.
    let target = *editor
        .committed_analysis(rate)
        .target()
        .ok_or("the edited recipe does not compile")?;
    let edited = editor.draft().snapshot();
    let sound = edited.at(rate)?;
    if sound.snapshot().stimulus() == snapshot.stimulus() {
        return Err("export did not capture the edited draft".into());
    }
    let path =
        std::env::temp_dir().join(format!("auditorylab-portable-{}.wav", std::process::id()));
    let exported = auditorylab::workflow::export(
        &sound,
        &path,
        Replace::Never,
        &Cancel::default(),
        &Progress::default(),
    );
    let bytes = std::fs::read(&path);
    let removed = std::fs::remove_file(&path);
    let exported = exported?;
    removed?;
    let bytes = bytes?;
    // Inspect the actual delivered samples, independently of the export call.
    let data = bytes
        .windows(4)
        .position(|chunk| chunk == b"data")
        .ok_or("WAV data missing")?
        + 8;
    for frame in 0..512_u64 {
        let start = data + usize::try_from(frame)? * 8;
        for (channel, expected) in sound.signal().frame(frame).iter().enumerate() {
            let offset = start + channel * 4;
            let actual = f32::from_le_bytes(bytes[offset..offset + 4].try_into()?);
            if actual != *expected as f32 {
                return Err("exported samples differ from edited signal".into());
            }
        }
    }

    // Live generation without a device.
    let mut generator = LiveGenerator::new(target);
    let mut buffer = [[0.0; 2]; 512];
    generator.render(&mut buffer);
    generator.control().update(&target)?;
    generator.stop();
    while !generator.is_finished() {
        generator.render(&mut buffer);
    }

    // The shared workspace has no device playback here.
    let mut workspace = Workspace::new(catalog, Capabilities::available())?;
    if workspace.capabilities().device_playback {
        return Err("device playback is offered without the device-playback feature".into());
    }
    if workspace.play() != Err(Refusal::DevicePlaybackUnavailable) || workspace.audio_busy() {
        return Err("audio was admitted without device playback".into());
    }
    workspace.shutdown()?;

    println!(
        "Portable consumer: edited, compiled, exported {} frames and generated live audio without a device.",
        exported.frames
    );
    Ok(())
}
