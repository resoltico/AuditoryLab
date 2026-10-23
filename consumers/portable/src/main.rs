//! Edit, compile, export and generate live audio with the library's default
//! features, and confirm the shared workspace refuses device playback.
use auditorylab::workspace::{Capabilities, Refusal, Workspace};
use auditorylab::{
    Cancel, Draft, EditingField, EditorSession, LiveGenerator, ParameterId, Progress, Replace,
    SampleRate,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let catalog = auditorylab::catalog()?;
    let snapshot = catalog.first().ok_or("empty catalog")?.clone();
    let rate = SampleRate::DEFAULT;

    // Editing: stage and commit through a field handle.
    let mut editor = EditorSession::new(Draft::from_snapshot(&snapshot))?;
    let (id, _) = *editor
        .draft()
        .components()
        .first()
        .ok_or("recipe without components")?;
    let level = editor.handle(EditingField::Parameter(id, ParameterId::PeakLevel));
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
    let sound = snapshot.at(rate)?;
    let path = std::env::temp_dir().join(format!("auditorylab-portable-{}.wav", std::process::id()));
    let exported = auditorylab::export(
        &sound,
        &path,
        Replace::Never,
        &Cancel::default(),
        &Progress::default(),
    );
    let removed = std::fs::remove_file(&path);
    let exported = exported?;
    removed?;

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
