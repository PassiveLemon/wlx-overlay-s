use std::{
    io::Read,
    os::unix::net::UnixStream,
    path::PathBuf,
    process::{Child, Command},
    sync::Arc,
};

use anyhow::Context;
use glam::Vec2;
use smithay::{
    backend::input::{ButtonState, Keycode},
    input::{
        keyboard::KeyboardHandle,
        pointer::{ButtonEvent, MotionEvent, PointerHandle},
    },
    reexports::wayland_server::{self, Resource, protocol::wl_surface::WlSurface},
    utils::{Logical, Point, SerialCounter},
};
use wgui::log::LogErr;
use xkbcommon::xkb;

use crate::backend::wayvr::{ExternalProcessRequest, WayVRTask};

use super::{
    ProcessWayVREnv,
    comp::{self, ClientState},
    process,
};

pub struct WayVRClient {
    pub client: wayland_server::Client,
    pub pid: u32,
}

pub struct WayVRCompositor {
    pub state: comp::Application,
    pub seat_keyboard: KeyboardHandle<comp::Application>,
    pub seat_pointer: PointerHandle<comp::Application>,
    pub serial_counter: SerialCounter,
    pub wayland_env: super::WaylandEnv,

    xwayland_satellite: Option<Child>,

    display: wayland_server::Display<comp::Application>,
    listener: wayland_server::ListeningSocket,

    toplevel_surf_count: u32, // for logging purposes

    pub clients: Vec<WayVRClient>,
}

impl Drop for WayVRCompositor {
    fn drop(&mut self) {
        if let Some(mut child) = self.xwayland_satellite.take() {
            unsafe {
                libc::kill(child.id() as i32, libc::SIGKILL);
            }
            // reap the pid
            let _ = child.wait();
        }
    }
}

fn get_wayvr_env_from_pid(pid: i32) -> anyhow::Result<ProcessWayVREnv> {
    let path = format!("/proc/{pid}/environ");
    let mut env_data = String::new();
    std::fs::File::open(path)?.read_to_string(&mut env_data)?;

    let lines: Vec<&str> = env_data.split('\0').filter(|s| !s.is_empty()).collect();

    let mut env = ProcessWayVREnv {
        display_auth: None,
        display_name: None,
    };

    for line in lines {
        if let Some((key, value)) = line.split_once('=') {
            if key == "WAYVR_DISPLAY_AUTH" {
                env.display_auth = Some(String::from(value));
            } else if key == "WAYVR_DISPLAY_NAME" {
                env.display_name = Some(String::from(value));
            }
        }
    }

    Ok(env)
}

impl WayVRCompositor {
    pub fn new(
        state: comp::Application,
        display: wayland_server::Display<comp::Application>,
        seat_keyboard: KeyboardHandle<comp::Application>,
        seat_pointer: PointerHandle<comp::Application>,
    ) -> anyhow::Result<Self> {
        let (wayland_env, listener) = create_wayland_listener()?;

        let xwayland_satellite = Command::new("xwayland-satellite")
            .arg(":20")
            .env("WAYLAND_DISPLAY", wayland_env.display_num_string())
            .spawn()
            .log_warn(
                "Could not start xwayland-satellite. Xwayland apps will not work in native mode",
            )
            .ok();

        Ok(Self {
            state,
            display,
            seat_keyboard,
            seat_pointer,
            listener,
            xwayland_satellite,
            wayland_env,
            serial_counter: SerialCounter::new(),
            clients: Vec::new(),
            toplevel_surf_count: 0,
        })
    }

    pub fn add_client(&mut self, client: WayVRClient) {
        self.clients.push(client);
    }

    pub fn cleanup_clients(&mut self) {
        self.clients.retain(|client| {
            let Some(data) = client.client.get_data::<ClientState>() else {
                return false;
            };

            if *data.disconnected.lock().unwrap() {
                return false;
            }

            true
        });
    }

    pub fn cleanup_handles(&mut self) {
        self.state.cleanup();
    }

    fn accept_connection(
        &mut self,
        stream: UnixStream,
        processes: &mut process::ProcessVec,
    ) -> anyhow::Result<()> {
        let client = self
            .display
            .handle()
            .insert_client(stream, Arc::new(comp::ClientState::default()))
            .unwrap();

        let creds = client.get_credentials(&self.display.handle())?;

        let process_env = get_wayvr_env_from_pid(creds.pid)?;

        // Find suitable auth key from the process list
        for p in processes.vec.iter().flatten() {
            if let process::Process::Managed(process) = &p.obj
                && let Some(auth_key) = &process_env.display_auth
            {
                // Find process with matching auth key
                if process.auth_key.as_str() == auth_key {
                    // Add client
                    self.add_client(WayVRClient {
                        client,
                        pid: creds.pid as u32,
                    });
                    return Ok(());
                }
            }
        }

        // This is a new process which we didn't met before.
        // Treat external processes exclusively (spawned by the user or external program)
        log::warn!(
            "External process ID {} connected to this Wayland server",
            creds.pid
        );

        self.state
            .wayvr_tasks
            .send(WayVRTask::NewExternalProcess(ExternalProcessRequest {
                env: process_env,
                client,
                pid: creds.pid as u32,
            }));

        Ok(())
    }

    fn accept_connections(&mut self, processes: &mut process::ProcessVec) -> anyhow::Result<()> {
        if let Some(stream) = self.listener.accept()?
            && let Err(e) = self.accept_connection(stream, processes)
        {
            log::error!("Failed to accept connection: {e}");
        }

        Ok(())
    }

    pub fn tick_wayland(&mut self, processes: &mut process::ProcessVec) -> anyhow::Result<()> {
        if let Err(e) = self.accept_connections(processes) {
            log::error!("accept_connections failed: {e}");
        }

        self.display.dispatch_clients(&mut self.state)?;
        self.display.flush_clients()?;

        let surf_count = self.state.xdg_shell.toplevel_surfaces().len() as u32;
        if surf_count != self.toplevel_surf_count {
            self.toplevel_surf_count = surf_count;
            log::info!("Toplevel surface count changed: {surf_count}");
        }

        Ok(())
    }

    pub fn send_key(&mut self, virtual_key: u32, down: bool) {
        let state = if down {
            smithay::backend::input::KeyState::Pressed
        } else {
            smithay::backend::input::KeyState::Released
        };

        self.seat_keyboard.input::<(), _>(
            &mut self.state,
            Keycode::new(virtual_key),
            state,
            self.serial_counter.next_serial(),
            0,
            |_, _, _| smithay::input::keyboard::FilterResult::Forward,
        );
    }

    pub fn set_keymap(&mut self, keymap: &xkb::Keymap) -> anyhow::Result<()> {
        // Smithay only accepts keymaps in a string form due to thread safety concerns
        self.seat_keyboard
            .set_keymap_from_string(
                &mut self.state,
                keymap.get_as_string(xkb::KEYMAP_FORMAT_USE_ORIGINAL),
            )
            .context("Failed to set keymap")
    }

    pub fn send_mouse_move_unfocused(&mut self, global_pos: Vec2) {
        let location: Point<f64, Logical> = (global_pos.x as f64, global_pos.y as f64).into();

        self.seat_pointer.motion(
            &mut self.state,
            None,
            &MotionEvent {
                location,
                serial: self.serial_counter.next_serial(),
                time: super::time::get_millis() as u32,
            },
        );

        self.seat_pointer.frame(&mut self.state);
    }

    pub fn send_mouse_move_to_surface(
        &mut self,
        surface: WlSurface,
        global_pos: Vec2,
        surface_origin: Vec2,
    ) {
        let location: Point<f64, Logical> = (global_pos.x as f64, global_pos.y as f64).into();

        let focus_location: Point<f64, Logical> =
            (surface_origin.x as f64, surface_origin.y as f64).into();

        let serial = self.serial_counter.next_serial();
        let time = super::time::get_millis() as u32;

        self.seat_pointer.motion(
            &mut self.state,
            Some((surface, focus_location)),
            &MotionEvent {
                location,
                serial,
                time,
            },
        );

        self.seat_pointer.frame(&mut self.state);
    }

    pub fn send_pointer_button(&mut self, index: super::MouseIndex, pressed: bool) {
        let state = if pressed {
            ButtonState::Pressed
        } else {
            ButtonState::Released
        };

        let serial = self.serial_counter.next_serial();
        let time = super::time::get_millis() as u32;
        let button = match index {
            super::MouseIndex::Left => 0x110,
            super::MouseIndex::Center => 0x112,
            super::MouseIndex::Right => 0x111,
        };

        log::trace!(
            "pointer button: button={button:#x} pressed={pressed} focus={:?}",
            self.seat_pointer.current_focus().map(|s| s.id())
        );

        self.seat_pointer.button(
            &mut self.state,
            &ButtonEvent {
                serial,
                time,
                button,
                state,
            },
        );

        self.seat_pointer.frame(&mut self.state);
    }
}

const STARTING_WAYLAND_ADDR_IDX: u32 = 20;

fn export_display_number(display_num: u32) -> anyhow::Result<()> {
    let mut path =
        std::env::var("XDG_RUNTIME_DIR").map_or_else(|_| PathBuf::from("/tmp"), PathBuf::from);
    path.push("wayvr.disp");
    std::fs::write(path, format!("{display_num}\n"))?;
    Ok(())
}

fn create_wayland_listener() -> anyhow::Result<(super::WaylandEnv, wayland_server::ListeningSocket)>
{
    let mut env = super::WaylandEnv {
        display_num: STARTING_WAYLAND_ADDR_IDX,
    };

    let listener = loop {
        let display_str = env.display_num_string();
        log::debug!("Trying to open socket \"{display_str}\"");
        match wayland_server::ListeningSocket::bind(display_str.as_str()) {
            Ok(listener) => {
                log::debug!("Listening to {display_str}");
                break listener;
            }
            Err(e) => {
                log::debug!(
                    "Failed to open socket \"{display_str}\" (reason: {e}), trying next..."
                );

                env.display_num += 1;
                if env.display_num > STARTING_WAYLAND_ADDR_IDX + 20 {
                    // Highly unlikely for the user to have 20 Wayland displays enabled at once. Return error instead.
                    anyhow::bail!("Failed to create wayland-server socket")
                }
            }
        }
    };

    if let Err(e) = export_display_number(env.display_num) {
        log::error!("Could not write wayvr.disp: {e:?}");
    }

    Ok((env, listener))
}
