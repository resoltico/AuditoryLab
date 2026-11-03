use super::*;
use DeviceAnswer as A;
use DeviceTest as T;

fn answered(answers: &[(DeviceTest, DeviceAnswer)]) -> DeviceCheck {
    let mut check = DeviceCheck::default();
    for (test, answer) in answers {
        check.record(*test, *answer).unwrap();
    }
    check
}

#[test]
fn every_test_recipe_is_one_quiet_tone_on_its_own_route() {
    for test in T::ALL {
        let recipe = test.recipe().unwrap();
        let components = recipe.content().sound.components();
        assert_eq!(components.len(), 1);
        let ComponentForm {
            source:
                SoundSource::Tone {
                    route,
                    peak_dbfs,
                    frequency_hz,
                },
            modulation: None,
        } = &components[0].form
        else {
            panic!("{test:?} must be an unmodulated tone");
        };
        assert_eq!(*route, test.route());
        assert_eq!(peak_dbfs.get(), -20.0);
        assert_eq!(frequency_hz.get(), test.frequency_hz());
        assert_eq!(recipe.content().timing.duration_seconds.get(), 3.0);
        assert_eq!(recipe.content().timing.fade_seconds.get(), 0.1);
    }
    assert_eq!(T::LeftChannel.route(), ChannelRouting::Left);
    assert_eq!(T::RightChannel.route(), ChannelRouting::Right);
    assert_eq!(T::Tone40Hz.frequency_hz(), 40.0);
}

#[test]
fn tests_refuse_answers_they_do_not_offer() {
    let mut check = DeviceCheck::default();
    assert_eq!(
        check.record(T::LeftChannel, A::Heard),
        Err(UnofferedAnswer {
            test: T::LeftChannel,
            answer: A::Heard
        })
    );
    assert!(check.record(T::Tone60Hz, A::Left).is_err());
    assert!(!check.started());
    check.record(T::Tone60Hz, A::Heard).unwrap();
    check.record(T::Tone60Hz, A::NotHeard).unwrap();
    assert_eq!(check.answer(T::Tone60Hz), Some(A::NotHeard));
    check.clear();
    assert_eq!(check, DeviceCheck::default());
}

#[test]
fn channel_answers_identify_separate_swapped_and_mixed_outputs() {
    let channels = |left, right| {
        answered(&[(T::LeftChannel, left), (T::RightChannel, right)])
            .assessment()
            .channels
    };
    assert_eq!(
        answered(&[(T::LeftChannel, A::Left)]).assessment().channels,
        ChannelResult::Incomplete
    );
    assert_eq!(channels(A::Left, A::Right), ChannelResult::Separate);
    assert_eq!(channels(A::Right, A::Left), ChannelResult::Swapped);
    assert_eq!(channels(A::Both, A::Both), ChannelResult::Mixed);
    assert_eq!(channels(A::NotHeard, A::NotHeard), ChannelResult::Silent);
    assert_eq!(
        channels(A::Left, A::NotHeard),
        ChannelResult::OneSide(T::LeftChannel)
    );
    assert_eq!(
        channels(A::NotHeard, A::Right),
        ChannelResult::OneSide(T::RightChannel)
    );
    assert_eq!(channels(A::Left, A::Left), ChannelResult::Inconsistent);
    assert_eq!(channels(A::Both, A::Right), ChannelResult::Inconsistent);
    assert_eq!(ChannelResult::Swapped.separates_ears(), Some(true));
    assert_eq!(ChannelResult::Mixed.separates_ears(), Some(false));
    assert_eq!(ChannelResult::Inconsistent.separates_ears(), None);
}

#[test]
fn low_tone_answers_report_the_lowest_heard_tone() {
    let low = |a100, a60, a40| {
        answered(&[(T::Tone100Hz, a100), (T::Tone60Hz, a60), (T::Tone40Hz, a40)])
            .assessment()
            .low_tones
    };
    assert_eq!(
        answered(&[(T::Tone100Hz, A::Heard)]).assessment().low_tones,
        LowToneResult::Incomplete
    );
    assert_eq!(low(A::Heard, A::Heard, A::Heard), LowToneResult::AllHeard);
    assert_eq!(
        low(A::Heard, A::Heard, A::NotHeard),
        LowToneResult::LowestHeard { test: T::Tone60Hz }
    );
    assert_eq!(
        low(A::Heard, A::NotHeard, A::Heard),
        LowToneResult::LowestHeard { test: T::Tone40Hz }
    );
    assert_eq!(
        low(A::NotHeard, A::NotHeard, A::NotHeard),
        LowToneResult::NoneHeard
    );
}
