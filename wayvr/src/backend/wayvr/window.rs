use std::rc::Rc;

use smithay::utils::{Logical, Size};
use smithay::wayland::shell::xdg::ToplevelSurface;
use wayvr_ipc::packet_server;

use crate::{backend::wayvr::process, gen_id};

#[derive(Debug)]
pub struct Window {
    pub min_size: Size<i32, Logical>,
    pub max_size: Size<i32, Logical>,
    pub bounds: Size<i32, Logical>,
    pub size_x: u32,
    pub size_y: u32,
    pub visible: bool,
    pub toplevel: Rc<ToplevelSurface>,
    pub process: process::ProcessHandle,
}

impl Window {
    const fn new(
        toplevel: Rc<ToplevelSurface>,
        process: process::ProcessHandle,
        bounds: Size<i32, Logical>,
        min_size: Size<i32, Logical>,
        max_size: Size<i32, Logical>,
    ) -> Self {
        Self {
            bounds,
            min_size,
            max_size,
            size_x: 0,
            size_y: 0,
            visible: true,
            toplevel,
            process,
        }
    }

    pub fn resizable(&self) -> bool {
        self.min_size != self.max_size
    }

    pub fn checked_configure_size(&mut self, size: Size<i32, Logical>) {
        let clamped_size = size.clamp(self.min_size, self.max_size);

        self.toplevel.with_pending_state(|state| {
            state.bounds = Some(self.bounds);
            state.size = Some(clamped_size);
        });
        self.toplevel.send_configure();
        self.remember_committed_size(size);
    }

    pub fn configure_size(&mut self, size: Option<Size<i32, Logical>>, bounds: Size<i32, Logical>) {
        self.toplevel.with_pending_state(|state| {
            state.bounds = Some(bounds);
            state.size = size;
        });
        self.toplevel.send_configure();

        self.bounds = bounds;
        if let Some(size) = size {
            self.remember_committed_size(size);
        }
    }

    pub fn request_size(&mut self, size: Size<i32, Logical>, bounds: Size<i32, Logical>) {
        let size = size.clamp(Size::new(1, 1), bounds);
        self.configure_size(Some(size), bounds);
    }

    pub fn remember_committed_size(&mut self, size: Size<i32, Logical>) -> bool {
        let size_x = size.w.max(1) as u32;
        let size_y = size.h.max(1) as u32;

        let changed = self.size_x != size_x || self.size_y != size_y;

        self.size_x = size_x;
        self.size_y = size_y;

        changed
    }
}

#[derive(Debug)]
pub struct MouseState {
    pub hover_window: WindowHandle,
    pub x: u32,
    pub y: u32,
}

#[derive(Debug)]
pub struct WindowManager {
    pub windows: WindowVec,
    pub mouse: Option<MouseState>,
}

impl WindowManager {
    pub const fn new() -> Self {
        Self {
            windows: WindowVec::new(),
            mouse: None,
        }
    }

    pub fn find_window_handle(&self, toplevel: &ToplevelSurface) -> Option<WindowHandle> {
        for (idx, cell) in self.windows.vec.iter().enumerate() {
            if let Some(cell) = cell {
                let window = &cell.obj;
                if *window.toplevel == *toplevel {
                    return Some(WindowVec::get_handle(cell, idx));
                }
            }
        }
        None
    }

    pub fn create_window(
        &mut self,
        toplevel: Rc<ToplevelSurface>,
        process: process::ProcessHandle,
        bounds: Size<i32, Logical>,
        min_size: Size<i32, Logical>,
        max_size: Size<i32, Logical>,
        size_x: u32,
        size_y: u32,
    ) -> WindowHandle {
        let mut window = Window::new(toplevel, process, bounds, min_size, max_size);
        window.remember_committed_size(Size::new(size_x as i32, size_y as i32));
        self.windows.add(window)
    }

    pub fn remove_window(&mut self, window_handle: WindowHandle) {
        self.windows.remove(&window_handle);
    }
}

gen_id!(WindowVec, Window, WindowCell, WindowHandle);

impl WindowHandle {
    pub const fn from_packet(handle: packet_server::WvrWindowHandle) -> Self {
        Self {
            generation: handle.generation,
            idx: handle.idx,
        }
    }

    pub const fn as_packet(&self) -> packet_server::WvrWindowHandle {
        packet_server::WvrWindowHandle {
            idx: self.idx,
            generation: self.generation,
        }
    }
}
