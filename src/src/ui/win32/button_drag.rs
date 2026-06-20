#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ButtonDragEndpoint {
    control_key: usize,
    tab_idx: usize,
    button_idx: usize,
    slot_idx: usize,
}

impl ButtonDragEndpoint {
    pub(crate) fn new(
        control_key: usize,
        tab_idx: usize,
        button_idx: usize,
        slot_idx: usize,
    ) -> Self {
        Self {
            control_key,
            tab_idx,
            button_idx,
            slot_idx,
        }
    }

    pub(crate) fn tab_idx(self) -> usize {
        self.tab_idx
    }

    pub(crate) fn button_idx(self) -> usize {
        self.button_idx
    }

    pub(crate) fn slot_idx(self) -> usize {
        self.slot_idx
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CursorPoint {
    pub(crate) x: i32,
    pub(crate) y: i32,
}

impl CursorPoint {
    pub(crate) fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }
}

#[derive(Debug, Default)]
pub(crate) struct ButtonDragController {
    state: Option<ButtonDragState>,
}

impl ButtonDragController {
    pub(crate) fn reset(&mut self) {
        self.state = None;
    }

    pub(crate) fn start(&mut self, source: ButtonDragEndpoint, point: CursorPoint) {
        self.state = Some(ButtonDragState {
            source,
            start: point,
            active: false,
        });
    }

    pub(crate) fn update(
        &mut self,
        source_key: usize,
        point: CursorPoint,
        threshold_px: i32,
    ) -> bool {
        let Some(state) = self.state.as_mut() else {
            return false;
        };
        if state.source.control_key != source_key {
            return false;
        }
        if !state.active && point_moved_far_enough(state.start, point, threshold_px) {
            state.active = true;
        }
        state.active
    }

    pub(crate) fn finish(&mut self, source_key: usize) -> Option<ButtonDragEndpoint> {
        let state = self.state.take()?;
        if state.source.control_key != source_key {
            self.state = Some(state);
            return None;
        }
        state.active.then_some(state.source)
    }

    pub(crate) fn cancel(&mut self, source_key: usize) -> bool {
        let Some(state) = self.state.take() else {
            return false;
        };
        if state.source.control_key != source_key {
            self.state = Some(state);
            return false;
        }
        state.active
    }
}

#[derive(Debug, Clone, Copy)]
struct ButtonDragState {
    source: ButtonDragEndpoint,
    start: CursorPoint,
    active: bool,
}

fn point_moved_far_enough(start: CursorPoint, current: CursorPoint, threshold_px: i32) -> bool {
    let threshold = i64::from(threshold_px.max(0));
    let dx = (i64::from(current.x) - i64::from(start.x)).abs();
    let dy = (i64::from(current.y) - i64::from(start.y)).abs();
    dx.max(dy) >= threshold
}

#[cfg(test)]
mod tests {
    use super::*;

    fn endpoint(key: usize) -> ButtonDragEndpoint {
        ButtonDragEndpoint::new(key, 0, key, key)
    }

    #[test]
    fn update_stays_inactive_until_threshold_is_reached() {
        let mut drag = ButtonDragController::default();
        drag.start(endpoint(1), CursorPoint::new(10, 10));

        assert!(!drag.update(1, CursorPoint::new(13, 10), 4));
        assert!(drag.update(1, CursorPoint::new(14, 10), 4));
        assert_eq!(drag.finish(1), Some(endpoint(1)));
    }

    #[test]
    fn finish_before_activation_preserves_click_behavior() {
        let mut drag = ButtonDragController::default();
        drag.start(endpoint(1), CursorPoint::new(10, 10));

        assert_eq!(drag.finish(1), None);
    }

    #[test]
    fn events_from_other_button_do_not_steal_active_drag() {
        let mut drag = ButtonDragController::default();
        drag.start(endpoint(1), CursorPoint::new(10, 10));

        assert!(!drag.update(2, CursorPoint::new(20, 10), 4));
        assert_eq!(drag.finish(2), None);
        assert!(!drag.cancel(2));
        assert!(drag.update(1, CursorPoint::new(20, 10), 4));
        assert_eq!(drag.finish(1), Some(endpoint(1)));
    }

    #[test]
    fn cancel_reports_whether_drag_was_active() {
        let mut drag = ButtonDragController::default();
        drag.start(endpoint(1), CursorPoint::new(10, 10));

        assert!(!drag.cancel(1));

        drag.start(endpoint(1), CursorPoint::new(10, 10));
        assert!(drag.update(1, CursorPoint::new(20, 10), 4));
        assert!(drag.cancel(1));
    }
}
