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

/// Where a cover sits relative to screen center, given its distance (in cards)
/// from the selected card. `x` is pixel offset from center; `scale` and
/// `opacity` fall off with distance.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CoverSlot {
    pub x: f32,
    pub scale: f32,
    pub opacity: f32,
}

pub const CARD_SPACING: f32 = 220.0;

pub fn slot_for(offset: i32) -> CoverSlot {
    let dist = offset.abs() as f32;
    let scale = if offset == 0 { 1.0 } else { 0.7 };
    let opacity = (1.0 - dist * 0.35).max(0.0);
    CoverSlot {
        x: offset as f32 * CARD_SPACING,
        scale,
        opacity,
    }
}

/// Cubic ease-out for selection slide. t in [0,1] -> [0,1].
pub fn ease_out_cubic(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    let p = 1.0 - t;
    1.0 - p * p * p
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

    #[test]
    fn center_slot_is_full() {
        let s = slot_for(0);
        assert_eq!(s.x, 0.0);
        assert_eq!(s.scale, 1.0);
        assert_eq!(s.opacity, 1.0);
    }

    #[test]
    fn neighbor_slots_are_symmetric_and_smaller() {
        let r = slot_for(1);
        let l = slot_for(-1);
        assert_eq!(r.x, CARD_SPACING);
        assert_eq!(l.x, -CARD_SPACING);
        assert_eq!(r.scale, l.scale);
        assert!(r.scale < 1.0);
        assert!(r.opacity < 1.0 && r.opacity > 0.0);
    }

    #[test]
    fn far_slots_fade_to_zero() {
        assert_eq!(slot_for(5).opacity, 0.0);
    }

    #[test]
    fn easing_endpoints_and_monotonic() {
        assert_eq!(ease_out_cubic(0.0), 0.0);
        assert_eq!(ease_out_cubic(1.0), 1.0);
        assert!(ease_out_cubic(0.5) > 0.5);
    }
}
