use crate::Result;
use crate::ui::WindowSpec;

mod geometry;

mod button_drag;

mod scan_worker;

mod native;

pub fn run_window(spec: WindowSpec) -> Result<()> {
    native::run_window(spec)
}
