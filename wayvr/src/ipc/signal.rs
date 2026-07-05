#[derive(Clone)]
pub enum WayVRSignal {
    BroadcastStateChanged(wayvr_ipc::packet_server::WvrStateChanged),
    DeviceHaptics(usize, crate::backend::input::Haptics),
    SwitchSet(Option<usize>),
    Handsfree(wayvr_ipc::packet_client::HandsfreeParams),
    ShowHide,
    CustomTask(crate::backend::task::ModifyPanelTask),
}
