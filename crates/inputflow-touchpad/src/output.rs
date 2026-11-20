/// Agent software-input marker shared with the probe's message recorder.
pub const AGENT_INPUT_TAG: usize = 0x494E_5055; // ASCII "INPU"

/// A click is submitted once. A partial submission may require one matching release.
pub struct InsertResult {
    pub requested: u32,
    pub inserted: u32,
    pub error: u32,
    pub repair: Option<(u32, u32)>,
}

pub fn insert_pair<T: Copy>(
    down: T,
    up: T,
    insert: impl FnMut(&[T]) -> (u32, u32),
) -> InsertResult {
    insert_chord(&[down, up], insert)
}

/// Submit downs then reverse-order ups. Recover only downs that were inserted
/// and whose corresponding releases were not inserted.
pub fn insert_chord<T>(inputs: &[T], mut insert: impl FnMut(&[T]) -> (u32, u32)) -> InsertResult {
    let requested = inputs.len() as u32;
    let (inserted, error) = insert(inputs);
    let held = inserted.min(requested.saturating_sub(inserted)) as usize;
    let repair = (held != 0).then(|| insert(&inputs[inputs.len() - held..]));
    InsertResult {
        requested,
        inserted,
        error,
        repair,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn partial_chord_repairs_only_still_held_keys_once() {
        let chord = [1, 2, 3, -3, -2, -1];
        for (accepted, repairs) in [
            (0, &[][..]),
            (1, &[-1][..]),
            (2, &[-2, -1]),
            (3, &[-3, -2, -1]),
            (4, &[-2, -1]),
            (5, &[-1]),
            (6, &[]),
        ] {
            let mut calls = Vec::new();
            let result = insert_chord(&chord, |inputs| {
                calls.push(inputs.to_vec());
                (
                    if calls.len() == 1 {
                        accepted
                    } else {
                        inputs.len() as u32
                    },
                    0,
                )
            });
            assert_eq!(calls[0], chord);
            assert_eq!(result.inserted, accepted);
            assert_eq!(result.requested, 6);
            if repairs.is_empty() {
                assert_eq!(calls.len(), 1);
            } else {
                assert_eq!(calls, [chord.to_vec(), repairs.to_vec()]);
            }
        }
    }
}
