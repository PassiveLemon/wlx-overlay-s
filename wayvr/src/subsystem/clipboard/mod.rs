#[cfg(feature = "wayland")]
pub mod wl;
#[cfg(feature = "x11")]
pub mod x11;

pub trait ClipboardProvider {
    fn set_clipboard_utf8(&mut self, content: &str);
}
