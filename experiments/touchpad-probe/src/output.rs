use std::mem::size_of;
use std::time::Instant;

use windows_sys::Win32::Foundation::GetLastError;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Mode {
    Observe,
    RightOnly,
    Both,
}

impl Mode {
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Observe => "observe",
            Self::RightOnly => "right_only",
            Self::Both => "both",
        }
    }
}

#[derive(Clone, Copy)]
pub(super) enum Button {
    Left,
    Right,
}

impl Button {
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Left => "left",
            Self::Right => "right",
        }
    }

    fn flags(self) -> (u32, u32) {
        match self {
            Self::Left => (MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP),
            Self::Right => (MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP),
        }
    }
}

#[derive(Clone, Copy)]
pub(super) struct Plan {
    pub token: usize,
    pub button: Button,
    pub criterion: Instant,
}

pub(super) const TOKEN_MASK: usize = 0xffff;
const TAG: usize = 0x496d_0000;

fn tag(token: usize) -> usize {
    TAG | (token & TOKEN_MASK)
}

pub(super) fn tagged_token(extra: usize) -> Option<usize> {
    (extra & 0xffff_0000 == TAG).then_some(extra & TOKEN_MASK)
}

pub(super) struct Result {
    pub inserted: u32,
    pub error: u32,
    pub repair: Option<(u32, u32)>,
}

pub(super) fn send(plan: Plan) -> Result {
    send_pair(plan.button, |flags| {
        let mut inputs = [INPUT::default(); 2];
        for (input, flag) in inputs.iter_mut().zip(flags) {
            *input = INPUT {
                r#type: INPUT_MOUSE,
                Anonymous: INPUT_0 {
                    mi: MOUSEINPUT {
                        dwFlags: *flag,
                        dwExtraInfo: tag(plan.token),
                        ..Default::default()
                    },
                },
            };
        }
        // SAFETY: A synchronous call with initialized INPUT records, confined by the caller's target check.
        let count = unsafe {
            SendInput(
                flags.len() as u32,
                inputs.as_ptr(),
                size_of::<INPUT>() as i32,
            )
        };
        let error = if count != flags.len() as u32 {
            unsafe { GetLastError() }
        } else {
            0
        };
        (count, error)
    })
}

fn send_pair(button: Button, mut insert: impl FnMut(&[u32]) -> (u32, u32)) -> Result {
    let (down, up) = button.flags();
    let (inserted, error) = insert(&[down, up]);
    // A partial insertion can leave our down outstanding. Attempt only its matching release.
    let repair = (inserted == 1).then(|| insert(&[up]));
    Result {
        inserted,
        error,
        repair,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn partial_insert_repairs_only_the_matching_release_once() {
        let mut calls = Vec::new();
        let result = send_pair(Button::Right, |flags| {
            calls.push(flags.to_vec());
            (1, 0)
        });
        assert_eq!(
            calls,
            [
                vec![MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP],
                vec![MOUSEEVENTF_RIGHTUP]
            ]
        );
        assert_eq!(result.repair, Some((1, 0)));
    }

    #[test]
    fn complete_and_zero_insert_have_no_repair_and_tag_round_trips() {
        for inserted in [0, 2] {
            assert!(send_pair(Button::Left, |_| (inserted, 0)).repair.is_none());
        }
        assert_eq!(tagged_token(tag(42)), Some(42));
        assert_eq!(tagged_token(0), None);
    }
}
