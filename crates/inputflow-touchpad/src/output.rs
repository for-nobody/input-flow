/// Agent software-input marker shared with the probe's message recorder.
pub const AGENT_INPUT_TAG: usize = 0x494E_5055; // ASCII "INPU"

/// A click is submitted once. A partial submission may require one matching release.
pub struct InsertResult {
    pub inserted: u32,
    pub error: u32,
    pub repair: Option<(u32, u32)>,
}

pub fn insert_pair<T: Copy>(
    down: T,
    up: T,
    mut insert: impl FnMut(&[T]) -> (u32, u32),
) -> InsertResult {
    let (inserted, error) = insert(&[down, up]);
    let repair = (inserted == 1).then(|| insert(&[up]));
    InsertResult {
        inserted,
        error,
        repair,
    }
}
