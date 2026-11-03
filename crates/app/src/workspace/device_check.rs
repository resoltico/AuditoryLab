//! Device-check answers and the captured test tones the check plays.
use super::{AudioRequest, Error, RecipeSnapshot, Refusal};
use crate::device_check::{DeviceAnswer, DeviceCheck, DeviceTest};

pub(super) struct DeviceCheckState {
    check: DeviceCheck,
    selected: DeviceTest,
    // One snapshot per test, in `DeviceTest::ALL` order, so previews and
    // playback of a test keep one identity for the session.
    tones: Vec<RecipeSnapshot>,
}
impl DeviceCheckState {
    pub(super) fn new() -> Result<Self, Error> {
        let tones = DeviceTest::ALL
            .into_iter()
            .map(DeviceTest::snapshot)
            .collect::<Result<_, Error>>()?;
        Ok(Self {
            check: DeviceCheck::default(),
            selected: DeviceTest::LeftChannel,
            tones,
        })
    }
    pub(super) const fn check(&self) -> &DeviceCheck {
        &self.check
    }
    pub(super) const fn selected(&self) -> DeviceTest {
        self.selected
    }
    pub(super) fn select(&mut self, test: DeviceTest) -> bool {
        std::mem::replace(&mut self.selected, test) != test
    }
    pub(super) fn tone(&self) -> &RecipeSnapshot {
        let index = DeviceTest::ALL
            .iter()
            .position(|test| *test == self.selected)
            .unwrap_or_else(|| unreachable!("every test has a tone"));
        &self.tones[index]
    }
    pub(super) fn record(&mut self, test: DeviceTest, answer: DeviceAnswer) -> Result<(), Refusal> {
        self.check
            .record(test, answer)
            .map_err(Refusal::UnofferedAnswer)
    }
    pub(super) fn clear(&mut self) -> bool {
        let started = self.check.started();
        self.check.clear();
        started
    }
    pub(super) fn prepare_audio(&self) -> AudioRequest {
        AudioRequest::Finite {
            snapshot: Box::new(self.tone().clone()),
            store: None,
        }
    }
}
