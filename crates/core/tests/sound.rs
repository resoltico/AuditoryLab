//! Checked authored identity/capacity boundaries and atomic edits.
use auditorylab_core::{
    Component, ComponentForm, ComponentId, Ears, Finite, Sound, StructureError,
};
const fn number(value: f64) -> Finite {
    match Finite::new(value) {
        Some(value) => value,
        None => panic!("finite fixture"),
    }
}
fn id(value: u32) -> ComponentId {
    ComponentId::new(value).unwrap_or_else(|| panic!("positive fixture id"))
}
fn tone(value: u32) -> Component {
    Component {
        id: id(value),
        form: ComponentForm::Tone {
            ears: Ears::Both,
            frequency_hz: number(200.0),
            peak_dbfs: number(-30.0),
            modulation: None,
        },
    }
}
fn pair(value: u32) -> Component {
    Component {
        id: id(value),
        form: ComponentForm::BinauralPair {
            left_hz: number(200.0),
            offset_hz: number(6.0),
            left_peak_dbfs: number(-30.0),
            right_peak_dbfs: number(-30.0),
        },
    }
}
#[test]
fn construction_rejects_duplicate_owners_and_both_independent_budgets() {
    assert_eq!(
        Sound::new(number(1.0), Finite::ZERO, vec![tone(7), tone(7)]),
        Err(StructureError::Identity { id: id(7) })
    );
    assert_eq!(
        Sound::new(number(1.0), Finite::ZERO, (1..=17).map(tone).collect()),
        Err(StructureError::ComponentBudget { count: 17 })
    );
    assert_eq!(
        Sound::new(number(1.0), Finite::ZERO, (1..=9).map(pair).collect()),
        Err(StructureError::VoiceBudget { count: 18 })
    );
}
#[test]
fn refused_push_insert_and_replacement_leave_the_entire_sound_unchanged() {
    let mut sound = Sound::new(number(1.0), Finite::ZERO, vec![tone(7)]).unwrap();
    let before = sound.clone();
    assert_eq!(
        sound.push(tone(7)),
        Err(StructureError::Identity { id: id(7) })
    );
    assert_eq!(sound, before);
    assert_eq!(
        sound.insert(0, tone(7)),
        Err(StructureError::Identity { id: id(7) })
    );
    assert_eq!(sound, before);
    let mut sound = Sound::new(number(1.0), Finite::ZERO, (1..=16).map(tone).collect()).unwrap();
    let before = sound.clone();
    assert_eq!(
        sound.replace(id(7), pair(7).form),
        Err(StructureError::VoiceBudget { count: 17 })
    );
    assert_eq!(sound, before);
    assert_eq!(
        sound.push(tone(17)),
        Err(StructureError::ComponentBudget { count: 17 })
    );
    assert_eq!(sound, before);
    assert_eq!(sound.replace(id(99), pair(99).form), Ok(false));
    assert_eq!(sound, before);
}
#[test]
fn edits_keep_owner_identity_order_and_signed_zero_content() {
    let mut sound = Sound::new(number(1.0), Finite::ZERO, vec![tone(7), tone(9)]).unwrap();
    let removed = sound.remove(id(7)).unwrap();
    assert_eq!(removed, (0, tone(7)));
    assert_eq!(sound.components(), &[tone(9)]);
    sound.insert(removed.0, removed.1).unwrap();
    assert_eq!(sound.components(), &[tone(7), tone(9)]);
    assert_eq!(sound.replace(id(7), pair(7).form), Ok(true));
    assert_eq!(sound.component(id(7)), Some(&pair(7)));
    let before = sound.clone();
    sound.set_fade(number(-0.0));
    assert_ne!(sound, before);
    assert_eq!(sound.fade_seconds().get().to_bits(), (-0.0_f64).to_bits());
    assert_eq!(sound.duration_seconds(), before.duration_seconds());
    assert_eq!(sound.components(), before.components());
    sound.set_duration(number(-0.0));
    assert_eq!(
        sound.duration_seconds().get().to_bits(),
        (-0.0_f64).to_bits()
    );
}
