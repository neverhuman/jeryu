#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavDirection {
    Up,
    Down,
    Left,
    Right,
}

impl NavDirection {
    pub fn is_horizontal(self) -> bool {
        matches!(self, NavDirection::Left | NavDirection::Right)
    }
}
