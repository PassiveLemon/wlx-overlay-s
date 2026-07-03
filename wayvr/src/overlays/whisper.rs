use std::{path::Path, rc::Rc, time::Duration};

use glam::{Affine3A, Quat, Vec3, vec3};
use wgui::{
    components::button::ComponentButton,
    event::EventCallback,
    i18n::Translation,
    log::LogErr,
    parser::Fetchable,
    widget::{EventResult, label::WidgetLabel},
};
use wlx_common::{
    data_dir,
    overlays::{BackendAttrib, BackendAttribValue, ToastTopic},
    windowing::{OverlayWindowState, Positioning},
};

use crate::{
    gui::{
        panel::{
            GuiPanel, NewGuiPanelParams, OnCustomAttribFunc,
            button::{BUTTON_EVENT_SUFFIX, BUTTON_EVENTS},
        },
        timer::GuiTimer,
    },
    overlays::toast::Toast,
    state::AppState,
    subsystem::{
        clipboard::{ClipboardProvider, wl::WlClipboardProvider, x11::X11ClipboardProvider},
        hid::VirtualKey,
        input::KeyboardFocus,
        whisper_stt::WhisperStt,
    },
    windowing::window::{OverlayCategory, OverlayWindowConfig},
};

pub const WHISPER_NAME: &str = "Speech-to-Text";

#[derive(Default)]
struct WhisperState {
    whisper_sst: Option<WhisperStt>,
    clipboard_provider: Option<Box<dyn ClipboardProvider>>,
    last_transcription: Option<Rc<str>>,
}

pub fn create_whisper(
    app: &mut AppState,
    headless: bool,
    wayland: bool,
) -> anyhow::Result<OverlayWindowConfig> {
    // let clipboard_provider: Option<Box<dyn ClipboardProvider>> = match (headless, wayland) {
    //     (true, _) => None,
    //     (false, true) => WlClipboardProvider::new()
    //         .log_err("Could not create Wayland clipboard provider")
    //         .ok()
    //         .map(|p| Box::new(p) as Box<dyn ClipboardProvider>),
    //     (false, false) => X11ClipboardProvider::new()
    //         .log_err("Could not create X11 clipboard provider")
    //         .ok()
    //         .map(|p| Box::new(p) as Box<dyn ClipboardProvider>),
    // };

    let state = WhisperState {
        clipboard_provider: None,
        ..Default::default()
    };
    let xml = "gui/whisper.xml";

    let on_custom_attrib: OnCustomAttribFunc = Box::new(move |layout, parser, attribs, _app| {
        let Ok(button) =
            parser.fetch_component_from_widget_id_as::<ComponentButton>(attribs.widget_id)
        else {
            return;
        };

        for (name, kind, test_button, test_duration) in &BUTTON_EVENTS {
            for suffix in BUTTON_EVENT_SUFFIX {
                let name = &format!("{name}{suffix}");
                let Some(action) = attribs.get_value(name) else {
                    break;
                };

                let mut args = action.split_whitespace();
                let Some(command) = args.next() else {
                    continue;
                };

                let button = button.clone();

                let callback: EventCallback<AppState, WhisperState> = match command {
                    "::WhisperTranscribeStart" => Box::new(move |_common, data, app, state| {
                        if !test_button(data) || !test_duration(&button, app) {
                            return Ok(EventResult::Pass);
                        }

                        if let Some(whisper) = state.whisper_sst.as_mut() {
                            let _ = whisper
                                .ptt_start()
                                .log_err("Could not start Whisper transcription");

                            return Ok(EventResult::Consumed);
                        }

                        let model_path = data_dir::get_path("whisper")
                            .join(app.session.config.whisper_model.as_ref());
                        if model_path.is_file() {
                            state.whisper_sst = WhisperStt::new(model_path)
                                .log_err("Could not create STT provider")
                                .ok();
                        } else {
                            Toast::new(
                                ToastTopic::System,
                                "WHISPER.MODEL_NOT_DOWNLOADED".into(),
                                "WHISPER.DOWNLOAD_GUIDANCE".into(),
                            )
                            .with_timeout(5.)
                            .with_sound(true)
                            .submit(app);
                        }

                        Ok(EventResult::Consumed)
                    }),
                    "::WhisperTranscribeStop" => Box::new(move |_common, data, app, state| {
                        if !test_button(data) || !test_duration(&button, app) {
                            return Ok(EventResult::Pass);
                        }

                        if let Some(whisper) = state.whisper_sst.as_mut() {
                            let _ = whisper
                                .ptt_end()
                                .log_err("Could not stop Whisper transcription");
                        }
                        Ok(EventResult::Consumed)
                    }),
                    "::WhisperPaste" => Box::new(move |_common, data, app, state| {
                        if !test_button(data) || !test_duration(&button, app) {
                            return Ok(EventResult::Pass);
                        }

                        let Some(transcription) = state.last_transcription.as_ref() else {
                            return Ok(EventResult::Consumed);
                        };

                        let mut success = false;

                        match app.hid_provider.keyboard_focus {
                            KeyboardFocus::WayVR => {
                                if let Some(wvr) = app.wvr_server.as_mut() {
                                    wvr.set_clipboard(transcription.as_ref());
                                    success = true;
                                }
                            }
                            KeyboardFocus::PhysicalScreen => {
                                if let Some(clip) = state.clipboard_provider.as_mut() {
                                    clip.set_clipboard_utf8(transcription.as_ref());
                                    success = true;
                                }
                            }
                        }

                        // send ctrl-v
                        if success {
                            app.hid_provider.send_key_routed(
                                app.wvr_server.as_mut(),
                                VirtualKey::RCtrl,
                                true,
                            );
                            app.hid_provider.send_key_routed(
                                app.wvr_server.as_mut(),
                                VirtualKey::V,
                                true,
                            );
                            app.hid_provider.send_key_routed(
                                app.wvr_server.as_mut(),
                                VirtualKey::RCtrl,
                                false,
                            );
                        }

                        Ok(EventResult::Consumed)
                    }),
                    "::WhisperUnload" => Box::new(move |_common, data, app, state| {
                        if !test_button(data) || !test_duration(&button, app) {
                            return Ok(EventResult::Pass);
                        }

                        state.whisper_sst = None;

                        Ok(EventResult::Consumed)
                    }),
                    _ => return,
                };

                let id = layout.add_event_listener(attribs.widget_id, *kind, callback);
                log::debug!("Registered {action} on {:?} as {id:?}", attribs.widget_id);
            }
        }
    });

    let params = NewGuiPanelParams {
        on_custom_attrib: Some(on_custom_attrib),
        ..Default::default()
    };

    let mut panel = GuiPanel::new_from_template(app, xml, state, params)?;
    panel.extra_attribs.insert(
        BackendAttrib::Icon,
        BackendAttribValue::Icon("icons/mic.svg".into()),
    );

    let label = panel.parser_state.get_widget_id("transcription")?;

    panel
        .timers
        .push(GuiTimer::new(Duration::from_millis(100), 0));

    let on_label_tick: EventCallback<AppState, WhisperState> =
        Box::new(move |common, data, _app, state| {
            if let Some(whisper_stt) = state.whisper_sst.as_mut()
                && let Some(text) = whisper_stt.take_transcription()
            {
                let text: Rc<str> = text.into();
                state.last_transcription = Some(text.clone());

                let label = data.obj.get_as_mut::<WidgetLabel>().unwrap();
                label.set_text(common, Translation::from_raw_text_rc(text));
            }

            Ok(EventResult::Pass)
        });

    panel.layout.add_event_listener(
        label,
        wgui::event::EventListenerKind::InternalStateChange,
        on_label_tick,
    );

    panel.update_layout(app)?;

    Ok(OverlayWindowConfig {
        name: WHISPER_NAME.into(),
        default_state: OverlayWindowState {
            interactable: true,
            grabbable: true,
            transform: Affine3A::from_scale_rotation_translation(
                Vec3::ONE * 0.5,
                Quat::IDENTITY,
                vec3(0.0, 0.0, -0.6),
            ),
            positioning: Positioning::Anchored,
            ..OverlayWindowState::default()
        },
        category: OverlayCategory::BuiltInPanel,
        ..OverlayWindowConfig::from_backend(Box::new(panel))
    })
}
