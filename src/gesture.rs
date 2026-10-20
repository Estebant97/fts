/// Pure Control+Space gesture state for the switcher.
/// Tracks modifiers and filters macOS key-repeat on Space.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GestureKey {
    ControlLeft,
    ControlRight,
    ShiftLeft,
    ShiftRight,
    Space,
    Escape,
    Other,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GestureInput {
    KeyPress(GestureKey),
    KeyRelease(GestureKey),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GestureAction {
    SpacePressed { reverse: bool },
    Cancel,
    CtrlReleased,
}

#[derive(Clone, Debug, Default)]
pub struct GestureState {
    ctrl_left: bool,
    ctrl_right: bool,
    shift_left: bool,
    shift_right: bool,
    /// True after the first Space key-down until Space is released.
    /// Suppresses autorepeat KeyPress events while held.
    space_held: bool,
}

impl GestureState {
    pub fn ctrl_down(&self) -> bool {
        self.ctrl_left || self.ctrl_right
    }

    pub fn shift_down(&self) -> bool {
        self.shift_left || self.shift_right
    }

    /// Handle one keyboard event.
    /// Returns `(action, swallow)` — when `swallow` is true the OS event
    /// should not be forwarded.
    pub fn handle(
        &mut self,
        input: GestureInput,
        session_active: bool,
    ) -> (Option<GestureAction>, bool) {
        match input {
            GestureInput::KeyPress(key) => self.handle_press(key, session_active),
            GestureInput::KeyRelease(key) => self.handle_release(key, session_active),
        }
    }

    fn handle_press(
        &mut self,
        key: GestureKey,
        session_active: bool,
    ) -> (Option<GestureAction>, bool) {
        match key {
            GestureKey::ControlLeft => {
                self.ctrl_left = true;
                (None, false)
            }
            GestureKey::ControlRight => {
                self.ctrl_right = true;
                (None, false)
            }
            GestureKey::ShiftLeft => {
                self.shift_left = true;
                (None, false)
            }
            GestureKey::ShiftRight => {
                self.shift_right = true;
                (None, false)
            }
            GestureKey::Space if self.ctrl_down() => {
                if self.space_held {
                    // Autorepeat while Space is held — ignore.
                    return (None, true);
                }
                self.space_held = true;
                (
                    Some(GestureAction::SpacePressed {
                        reverse: self.shift_down(),
                    }),
                    true,
                )
            }
            GestureKey::Escape if session_active => (Some(GestureAction::Cancel), true),
            _ => (None, false),
        }
    }

    fn handle_release(
        &mut self,
        key: GestureKey,
        session_active: bool,
    ) -> (Option<GestureAction>, bool) {
        match key {
            GestureKey::ControlLeft => {
                let was_down = self.ctrl_down();
                self.ctrl_left = false;
                let action = if was_down && !self.ctrl_down() {
                    Some(GestureAction::CtrlReleased)
                } else {
                    None
                };
                (action, false)
            }
            GestureKey::ControlRight => {
                let was_down = self.ctrl_down();
                self.ctrl_right = false;
                let action = if was_down && !self.ctrl_down() {
                    Some(GestureAction::CtrlReleased)
                } else {
                    None
                };
                (action, false)
            }
            GestureKey::ShiftLeft => {
                self.shift_left = false;
                (None, false)
            }
            GestureKey::ShiftRight => {
                self.shift_right = false;
                (None, false)
            }
            GestureKey::Space => {
                self.space_held = false;
                let swallow = self.ctrl_down() || session_active;
                (None, swallow)
            }
            GestureKey::Escape if session_active => (None, true),
            _ => (None, false),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{GestureAction, GestureInput, GestureKey, GestureState};

    #[test]
    fn first_space_press_emits_action() {
        let mut state = GestureState::default();
        state.handle(GestureInput::KeyPress(GestureKey::ControlLeft), false);

        let (action, swallow) = state.handle(GestureInput::KeyPress(GestureKey::Space), false);
        assert_eq!(action, Some(GestureAction::SpacePressed { reverse: false }));
        assert!(swallow);
    }

    #[test]
    fn space_autorepeat_is_ignored() {
        let mut state = GestureState::default();
        state.handle(GestureInput::KeyPress(GestureKey::ControlLeft), false);
        let _ = state.handle(GestureInput::KeyPress(GestureKey::Space), false);

        let (action, swallow) = state.handle(GestureInput::KeyPress(GestureKey::Space), true);
        assert_eq!(action, None);
        assert!(swallow);

        let (action, swallow) = state.handle(GestureInput::KeyPress(GestureKey::Space), true);
        assert_eq!(action, None);
        assert!(swallow);
    }

    #[test]
    fn space_after_release_emits_again() {
        let mut state = GestureState::default();
        state.handle(GestureInput::KeyPress(GestureKey::ControlLeft), false);
        let _ = state.handle(GestureInput::KeyPress(GestureKey::Space), false);
        let _ = state.handle(GestureInput::KeyRelease(GestureKey::Space), true);

        let (action, _) = state.handle(GestureInput::KeyPress(GestureKey::Space), true);
        assert_eq!(action, Some(GestureAction::SpacePressed { reverse: false }));
    }

    #[test]
    fn shift_space_is_reverse() {
        let mut state = GestureState::default();
        state.handle(GestureInput::KeyPress(GestureKey::ControlLeft), false);
        state.handle(GestureInput::KeyPress(GestureKey::ShiftLeft), false);

        let (action, _) = state.handle(GestureInput::KeyPress(GestureKey::Space), false);
        assert_eq!(action, Some(GestureAction::SpacePressed { reverse: true }));
    }

    #[test]
    fn right_shift_also_reverses() {
        let mut state = GestureState::default();
        state.handle(GestureInput::KeyPress(GestureKey::ControlRight), false);
        state.handle(GestureInput::KeyPress(GestureKey::ShiftRight), false);

        let (action, _) = state.handle(GestureInput::KeyPress(GestureKey::Space), false);
        assert_eq!(action, Some(GestureAction::SpacePressed { reverse: true }));
    }

    #[test]
    fn unrelated_keys_are_ignored() {
        let mut state = GestureState::default();
        let (action, swallow) = state.handle(GestureInput::KeyPress(GestureKey::Other), false);
        assert_eq!(action, None);
        assert!(!swallow);
    }

    #[test]
    fn ctrl_release_emits_when_last_control_goes_up() {
        let mut state = GestureState::default();
        state.handle(GestureInput::KeyPress(GestureKey::ControlLeft), false);
        state.handle(GestureInput::KeyPress(GestureKey::ControlRight), false);

        let (action, _) = state.handle(GestureInput::KeyRelease(GestureKey::ControlLeft), true);
        assert_eq!(action, None);

        let (action, _) = state.handle(GestureInput::KeyRelease(GestureKey::ControlRight), true);
        assert_eq!(action, Some(GestureAction::CtrlReleased));
    }

    #[test]
    fn escape_cancels_active_session() {
        let mut state = GestureState::default();
        let (action, swallow) = state.handle(GestureInput::KeyPress(GestureKey::Escape), true);
        assert_eq!(action, Some(GestureAction::Cancel));
        assert!(swallow);
    }
}
