use smithay_clipboard::Clipboard;
use wlx_capture::wayland::smithay_client_toolkit::reexports::client::{Connection, Proxy};

use crate::subsystem::clipboard::ClipboardProvider;

pub struct WlClipboardProvider {
    clipboard: Clipboard,
}

impl WlClipboardProvider {
    pub fn new() -> anyhow::Result<Self> {
        let connection = Connection::connect_to_env()?;
        let clipboard = unsafe { Clipboard::new(connection.display().id().as_ptr() as *mut _) };

        Ok(Self { clipboard })
    }
}

impl ClipboardProvider for WlClipboardProvider {
    fn set_clipboard_utf8(&mut self, content: &str) {
        self.clipboard.store(content.to_owned());
    }
}
