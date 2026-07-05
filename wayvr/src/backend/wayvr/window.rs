use std::rc::Rc;

use smithay::wayland::shell::xdg::ToplevelSurface;
use wayvr_ipc::packet_server;

use crate::{backend::wayvr::process, gen_id};

#[derive(Debug)]
pub struct Window {
    pub size_x: u32,
    pub size_y: u32,
    pub visible: bool,
    pub toplevel: Rc<ToplevelSurface>,
    pub process: process::ProcessHandle,
}

impl Window {
    const fn new(toplevel: Rc<ToplevelSurface>, process: process::ProcessHandle) -> Self {
        Self {
            size_x: 0,
            size_y: 0,
            visible: true,
            toplevel,
            process,
        }
    }

    pub fn set_size(&mut self, size_x: u32, size_y: u32) {
        self.toplevel.with_pending_state(|state| {
            //state.bounds = Some((size_x as i32, size_y as i32).into());
            state.size = Some((size_x as i32, size_y as i32).into());
        });
        self.toplevel.send_configure();

        self.size_x = size_x;
        self.size_y = size_y;
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
        size_x: u32,
        size_y: u32,
    ) -> WindowHandle {
        let mut window = Window::new(toplevel, process);
        window.set_size(size_x, size_y);
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
