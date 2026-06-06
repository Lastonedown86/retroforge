/// A single horizontal coverflow row. Selection is clamped (no wrap) for the slice.
#[derive(Debug, Clone, PartialEq)]
pub struct Shelf {
    pub len: usize,
    pub selected: usize,
}

impl Shelf {
    pub fn new(len: usize) -> Self {
        Shelf { len, selected: 0 }
    }
    pub fn move_left(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }
    pub fn move_right(&mut self) {
        if self.len > 0 && self.selected + 1 < self.len {
            self.selected += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_starts_at_zero() {
        let s = Shelf::new(5);
        assert_eq!(s.selected, 0);
        assert_eq!(s.len, 5);
    }

    #[test]
    fn move_right_advances_then_clamps() {
        let mut s = Shelf::new(3);
        s.move_right();
        assert_eq!(s.selected, 1);
        s.move_right();
        assert_eq!(s.selected, 2);
        s.move_right(); // clamp at len-1
        assert_eq!(s.selected, 2);
    }

    #[test]
    fn move_left_clamps_at_zero() {
        let mut s = Shelf::new(3);
        s.move_left();
        assert_eq!(s.selected, 0);
        s.move_right();
        s.move_left();
        assert_eq!(s.selected, 0);
    }
}
