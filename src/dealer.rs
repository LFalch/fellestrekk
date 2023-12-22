#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Dealer {
    /// If false, stands on soft 17, if true, hits on soft 17
    pub (in super) hit_soft_17: bool,
}

impl Dealer {
    /// Dealers that stands on soft 17
    pub fn s17() -> Self {
        Dealer { hit_soft_17: false }
    }
    /// Dealers that hits on soft 17
    pub fn h17() -> Self {
        Dealer { hit_soft_17: true }
    }
}
