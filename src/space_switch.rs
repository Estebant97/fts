//! Sequence a Dock-owned transition; never infer completion from CG window visibility.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpaceSwitch {
    AlreadyActive,
    Switched,
}

pub trait SpaceTransition {
    type Error;
    fn is_current(&mut self) -> Result<bool, Self::Error>;
    fn press_destination(&mut self) -> Result<(), Self::Error>;
    fn wait(&mut self);
    fn timeout(&self) -> Self::Error;
}

pub fn switch_space<T: SpaceTransition>(transition: &mut T) -> Result<SpaceSwitch, T::Error> {
    if transition.is_current()? {
        return Ok(SpaceSwitch::AlreadyActive);
    }
    transition.press_destination()?;
    for _ in 0..40 {
        transition.wait();
        if transition.is_current()? {
            return Ok(SpaceSwitch::Switched);
        }
    }
    Err(transition.timeout())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;

    struct Fake {
        current: VecDeque<Result<bool, &'static str>>,
        presses: usize,
        waits: usize,
        press_error: Option<&'static str>,
    }

    impl Fake {
        fn new(current: &[Result<bool, &'static str>]) -> Self {
            Self {
                current: current.iter().copied().collect(),
                presses: 0,
                waits: 0,
                press_error: None,
            }
        }
    }

    impl SpaceTransition for Fake {
        type Error = &'static str;
        fn is_current(&mut self) -> Result<bool, Self::Error> {
            self.current.pop_front().unwrap_or(Ok(false))
        }
        fn press_destination(&mut self) -> Result<(), Self::Error> {
            self.presses += 1;
            self.press_error.map_or(Ok(()), Err)
        }
        fn wait(&mut self) {
            self.waits += 1;
        }
        fn timeout(&self) -> Self::Error {
            "timeout"
        }
    }

    #[test]
    fn active_destination_does_not_open_mission_control() {
        let mut t = Fake::new(&[Ok(true)]);
        assert_eq!(switch_space(&mut t), Ok(SpaceSwitch::AlreadyActive));
        assert_eq!((t.presses, t.waits), (0, 0));
    }

    #[test]
    fn waits_for_destination_confirmation_before_success() {
        let mut t = Fake::new(&[Ok(false), Ok(false), Ok(false), Ok(true)]);
        assert_eq!(switch_space(&mut t), Ok(SpaceSwitch::Switched));
        assert_eq!((t.presses, t.waits), (1, 3));
    }

    #[test]
    fn successful_press_without_transition_is_an_error() {
        let mut t = Fake::new(&[]);
        assert_eq!(switch_space(&mut t), Err("timeout"));
        assert_eq!((t.presses, t.waits), (1, 40));
    }

    #[test]
    fn failed_press_does_not_report_success_or_retry_a_toggle() {
        let mut t = Fake::new(&[Ok(false)]);
        t.press_error = Some("AXPress failed");
        assert_eq!(switch_space(&mut t), Err("AXPress failed"));
        assert_eq!((t.presses, t.waits), (1, 0));
    }

    #[test]
    fn read_failure_is_not_treated_as_an_empty_space() {
        let mut t = Fake::new(&[Err("no display")]);
        assert_eq!(switch_space(&mut t), Err("no display"));
        assert_eq!(t.presses, 0);
    }

    #[test]
    fn repeated_switches_each_wait_for_their_own_destination() {
        let mut t = Fake::new(&[
            Ok(false),
            Ok(true),
            Ok(false),
            Ok(false),
            Ok(true),
            Ok(true),
        ]);
        assert_eq!(switch_space(&mut t), Ok(SpaceSwitch::Switched));
        assert_eq!(switch_space(&mut t), Ok(SpaceSwitch::Switched));
        assert_eq!(switch_space(&mut t), Ok(SpaceSwitch::AlreadyActive));
        assert_eq!((t.presses, t.waits), (2, 3));
    }
}
