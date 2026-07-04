use glam::{Affine2, Affine3A, Quat, Vec2, Vec3, vec2, vec3};
use smithay::{
    desktop::PopupManager,
    reexports::wayland_server::{Resource, backend::ObjectId, protocol::wl_surface::WlSurface},
    utils::{Logical, Point},
    wayland::{
        compositor::{
            SUBSURFACE_ROLE, SubsurfaceCachedState, SurfaceAttributes, TraversalAction,
            with_states, with_surface_tree_upward,
        },
        shell::xdg::XdgPopupSurfaceData,
    },
};
use std::{mem, ops::RangeInclusive, sync::Arc};
use vulkano::{
    buffer::BufferUsage, image::view::ImageView, pipeline::graphics::color_blend::AttachmentBlend,
};
use wayvr_ipc::packet_client::PositionMode;
use wgui::{
    components::button::ComponentButton,
    event::EventCallback,
    gfx::{
        cmd::WGfxClearMode,
        pipeline::{WGfxPipeline, WPipelineCreateInfo},
    },
    i18n::Translation,
    parser::Fetchable,
    widget::{EventResult, label::WidgetLabel},
};
use wlx_capture::frame::{MouseMeta, Transform};
use wlx_common::{
    overlays::{BackendAttrib, BackendAttribValue, StereoMode},
    windowing::{OverlayWindowState, Positioning},
};

use crate::{
    backend::{
        XrBackend,
        input::{self, HoverResult},
        wayvr::{self, SurfaceBufWithImage, process::KillSignal, window::WindowHandle},
    },
    graphics::{ExtentExt, Vert2Uv, upload_quad_vertices},
    gui::panel::{
        GuiPanel, NewGuiPanelParams, OnCustomAttribFunc,
        button::{BUTTON_EVENT_SUFFIX, BUTTON_EVENTS},
    },
    overlays::screen::capture::ScreenPipeline,
    state::{self, AppState},
    subsystem::{hid::WheelDelta, input::KeyboardFocus},
    windowing::{
        backend::{
            FrameMeta, OverlayBackend, OverlayEventData, RenderResources, ShouldRender,
            ui_transform,
        },
        window::{OverlayCategory, OverlayWindowConfig},
    },
};

pub enum WvrCommand {
    CloseWindow,
    KillProcess(KillSignal),
}

const BORDER_SIZE: u32 = 5;
const BAR_SIZE: u32 = 48;

pub fn create_wl_window_overlay(
    name: Arc<str>,
    app: &mut AppState,
    window: wayvr::window::WindowHandle,
    icon: Arc<str>,
    size: [u32; 2],
    pos_mode: PositionMode,
) -> anyhow::Result<OverlayWindowConfig> {
    let scale = size[0].max(size[1]) as f32 / 1920.0;
    let curve_scale = size[0] as f32 / 1920.0;

    let z_dist = if matches!(pos_mode, PositionMode::Anchor) {
        0.0
    } else {
        -0.95
    };

    Ok(OverlayWindowConfig {
        name: name.clone(),
        default_state: OverlayWindowState {
            grabbable: true,
            interactable: true,
            positioning: match pos_mode {
                PositionMode::Float => Positioning::Floating,
                PositionMode::Anchor => Positioning::Anchored,
                PositionMode::Static => Positioning::Static,
            },
            curvature: Some(0.15 * curve_scale),
            transform: Affine3A::from_scale_rotation_translation(
                Vec3::ONE * scale,
                Quat::IDENTITY,
                vec3(0.0, 0.0, z_dist),
            ),
            ..OverlayWindowState::default()
        },
        keyboard_focus: Some(KeyboardFocus::WayVR),
        category: OverlayCategory::WayVR,
        show_on_spawn: true,
        ..OverlayWindowConfig::from_backend(Box::new(WvrWindowBackend::new(
            name, app, window, icon,
        )?))
    })
}

#[derive(Clone)]
struct RenderedSurface {
    surface: WlSurface,
    surface_id: ObjectId,
    image: Arc<ImageView>,
    pos: Vec2,
    size: Vec2,
}

enum WvrHitTarget {
    Panel(input::PointerHit),
    Toplevel {
        pos: Vec2,
    },
    Surface {
        surface: WlSurface,
        global_pos: Vec2,
        surface_origin: Vec2,
    },
    Popup {
        surface: WlSurface,
        global_pos: Vec2,
        surface_origin: Vec2,
    },
}

pub struct WvrWindowBackend {
    name: Arc<str>,
    icon: Arc<str>,
    pipeline: Option<ScreenPipeline>,
    subsurface_pipeline: Arc<WGfxPipeline<Vert2Uv>>,
    popup_outside_button: Option<wayvr::MouseIndex>,
    interaction_transform: Option<Affine2>,
    window: WindowHandle,
    popups: Vec<RenderedSurface>,
    surfaces: Vec<RenderedSurface>,
    just_resumed: bool,
    meta: Option<FrameMeta>,
    mouse: Option<MouseMeta>,
    stereo: Option<StereoMode>,
    stereo_full_frame: bool,
    stereo_adjust_mouse: bool,
    cur_image: Option<Arc<ImageView>>,
    panel: GuiPanel<WindowHandle>,
    inner_extent: [u32; 2],
    mouse_transform: Affine2,
    uv_range: RangeInclusive<f32>,
    panel_hovered: bool,
}

impl WvrWindowBackend {
    fn new(
        name: Arc<str>,
        app: &mut AppState,
        window: wayvr::window::WindowHandle,
        icon: Arc<str>,
    ) -> anyhow::Result<Self> {
        let subsurface_pipeline = app.gfx.create_pipeline(
            app.gfx_extras.shaders.get("vert_quad").unwrap(), // want panic
            app.gfx_extras.shaders.get("frag_simple").unwrap(), // want panic
            WPipelineCreateInfo::new(app.gfx.surface_format).use_blend(AttachmentBlend::alpha()),
        )?;

        let on_custom_attrib: OnCustomAttribFunc =
            Box::new(move |layout, parser, attribs, _app| {
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

                        let callback: EventCallback<AppState, WindowHandle> = match command {
                            "::DecorCloseWindow" => Box::new(move |_common, data, app, state| {
                                if !test_button(data) || !test_duration(&button, app) {
                                    return Ok(EventResult::Pass);
                                }

                                app.wvr_server.as_mut().unwrap().close_window(*state);

                                Ok(EventResult::Consumed)
                            }),
                            _ => return,
                        };

                        let id = layout.add_event_listener(attribs.widget_id, *kind, callback);
                        log::debug!("Registered {action} on {:?} as {id:?}", attribs.widget_id);
                    }
                }
            });

        let mut panel = GuiPanel::new_from_template(
            app,
            "gui/decor.xml",
            window,
            NewGuiPanelParams {
                resize_to_parent: true,
                on_custom_attrib: Some(on_custom_attrib),
                ..Default::default()
            },
        )?;

        {
            let mut title = panel
                .parser_state
                .fetch_widget_as::<WidgetLabel>(&panel.layout.state, "label_title")?;
            title.set_text_simple(
                &mut app.wgui_globals.get(),
                Translation::from_raw_text(&name),
            );
        }

        panel.update_layout(app)?;

        Ok(Self {
            name,
            icon,
            pipeline: None,
            window,
            popups: vec![],
            surfaces: vec![],
            subsurface_pipeline,
            popup_outside_button: None,
            interaction_transform: None,
            just_resumed: false,
            meta: None,
            mouse: None,
            stereo: if matches!(app.xr_backend, XrBackend::OpenXR) {
                Some(StereoMode::None)
            } else {
                None
            },
            stereo_full_frame: false,
            stereo_adjust_mouse: false,
            cur_image: None,
            inner_extent: [0, 0],
            panel,
            mouse_transform: Affine2::ZERO,
            uv_range: 0.0..=1.0,
            panel_hovered: false,
        })
    }

    fn apply_extent(&mut self, app: &mut AppState, meta: &FrameMeta) -> anyhow::Result<()> {
        self.interaction_transform = Some(ui_transform(meta.extent));

        let mut scale = vec2(
            ((meta.extent[0] + BORDER_SIZE * 2) as f32) / (meta.extent[0] as f32),
            ((meta.extent[1] + BORDER_SIZE * 2 + BAR_SIZE) as f32) / (meta.extent[1] as f32),
        );

        if self.stereo_adjust_mouse
            && let Some(stereo) = self.stereo
        {
            match stereo {
                StereoMode::LeftRight | StereoMode::RightLeft => scale.x *= 0.5,
                StereoMode::TopBottom | StereoMode::BottomTop => scale.y *= 0.5,
                _ => {}
            }
        }

        let translation = vec2(
            -(BORDER_SIZE as f32) / (meta.extent[0] as f32),
            -((BORDER_SIZE + BAR_SIZE) as f32) / (meta.extent[1] as f32),
        );

        self.mouse_transform = Affine2::from_scale_angle_translation(scale, 0.0, translation);
        self.uv_range = translation[0]..=(1.0 - translation[0]);

        self.panel.max_size = vec2(
            (meta.extent[0]/*  + BORDER_SIZE * 2 (disabled for now) */) as _,
            BAR_SIZE as _,
        );
        self.panel.update_layout(app)?;

        Ok(())
    }

    fn transformed_uv_from_hit(&self, hit: &input::PointerHit) -> Vec2 {
        self.mouse_transform.transform_point2(hit.uv)
    }

    fn client_pos_from_transformed_uv(&self, transformed: Vec2) -> Vec2 {
        vec2(
            transformed.x * self.inner_extent[0] as f32,
            transformed.y * self.inner_extent[1] as f32,
        )
    }

    fn unclamped_client_pos_from_hit(&self, hit: &input::PointerHit) -> Vec2 {
        let transformed = self.transformed_uv_from_hit(hit);
        self.client_pos_from_transformed_uv(transformed)
    }

    fn is_inside_client_area(&self, transformed: Vec2) -> bool {
        self.uv_range.contains(&transformed.x) && self.uv_range.contains(&transformed.y)
    }

    fn panel_hit_from_hit(&self, hit: &input::PointerHit) -> Option<input::PointerHit> {
        let meta = self.meta.as_ref()?;

        let panel_height = meta.extent[1].checked_sub(self.inner_extent[1])?;
        if panel_height == 0 {
            return None;
        }

        let mut hit2 = *hit;
        hit2.uv.y *= meta.extent[1] as f32 / panel_height as f32;

        Some(hit2)
    }

    fn popup_hit_at_client_pos(&self, pos: Vec2) -> Option<(WlSurface, Vec2)> {
        self.popups.iter().rev().find_map(|popup| {
            let local = pos - popup.pos;

            let hit = local.x >= 0.0
                && local.y >= 0.0
                && local.x < popup.size.x
                && local.y < popup.size.y;

            if hit {
                Some((popup.surface.clone(), popup.pos))
            } else {
                None
            }
        })
    }

    fn hit_target(&self, hit: &input::PointerHit) -> Option<WvrHitTarget> {
        let transformed = self.transformed_uv_from_hit(hit);
        let client_pos = self.client_pos_from_transformed_uv(transformed);

        // popups are checked before the panel/client bounds.
        if let Some((surface, surface_origin)) = self.popup_hit_at_client_pos(client_pos) {
            return Some(WvrHitTarget::Popup {
                surface,
                global_pos: client_pos,
                surface_origin,
            });
        }

        if !self.is_inside_client_area(transformed) {
            return self.panel_hit_from_hit(hit).map(WvrHitTarget::Panel);
        }

        let hit_surface = self
            .surfaces
            .iter()
            .rev()
            .find(|s| surface_accepts_input(s, client_pos));

        if let Some(surface) = hit_surface {
            return Some(WvrHitTarget::Surface {
                surface: surface.surface.clone(),
                global_pos: client_pos,
                surface_origin: surface.pos,
            });
        }

        let clamped = transformed.clamp(Vec2::ZERO, Vec2::ONE);
        let pos = self.client_pos_from_transformed_uv(clamped);

        Some(WvrHitTarget::Toplevel { pos })
    }

    fn mouse_index_from_mode(mode: input::PointerMode) -> Option<wayvr::MouseIndex> {
        match mode {
            input::PointerMode::Left => Some(wayvr::MouseIndex::Left),
            input::PointerMode::Middle => Some(wayvr::MouseIndex::Center),
            input::PointerMode::Right => Some(wayvr::MouseIndex::Right),
            _ => None,
        }
    }

    fn render_subsurface(
        &self,
        app: &mut AppState,
        rdr: &mut RenderResources,
        s: &RenderedSurface,
    ) -> anyhow::Result<()> {
        let meta = self.meta.as_ref().unwrap();
        let extentf = [meta.extent[0] as f32, meta.extent[1] as f32];

        let mut buf_vert = app
            .gfx
            .empty_buffer(BufferUsage::TRANSFER_DST | BufferUsage::VERTEX_BUFFER, 4)?;

        upload_quad_vertices(
            &mut buf_vert,
            extentf[0],
            extentf[1],
            s.pos.x,
            s.pos.y,
            s.size.x,
            s.size.y,
        )?;

        let set0 =
            self.subsurface_pipeline
                .uniform_sampler(0, s.image.clone(), app.gfx.texture_filter)?;

        let pass = self.subsurface_pipeline.create_pass(
            extentf,
            [BORDER_SIZE as _, (BAR_SIZE + BORDER_SIZE) as _],
            buf_vert,
            0..4,
            0..1,
            vec![set0],
            &Default::default(),
        )?;

        for buf in &mut rdr.cmd_bufs {
            buf.run_ref(&pass)?;
        }

        Ok(())
    }
}

impl OverlayBackend for WvrWindowBackend {
    fn init(&mut self, app: &mut state::AppState) -> anyhow::Result<()> {
        self.panel.init(app)
    }

    fn pause(&mut self, app: &mut state::AppState) -> anyhow::Result<()> {
        self.panel.pause(app)
    }

    fn resume(&mut self, app: &mut state::AppState) -> anyhow::Result<()> {
        self.just_resumed = true;
        self.panel.resume(app)
    }

    #[allow(clippy::too_many_lines)]
    fn should_render(&mut self, app: &mut AppState) -> anyhow::Result<ShouldRender> {
        let Some(toplevel) = app
            .wvr_server
            .as_ref()
            .and_then(|sv| sv.wm.windows.get(&self.window))
            .map(|win| win.toplevel.clone())
        else {
            log::debug!(
                "{:?}: WayVR overlay without matching window entry",
                self.name
            );
            return Ok(ShouldRender::Unable);
        };

        let surface_id = toplevel.wl_surface().id();
        let surfaces = collect_rendered_surface_tree(toplevel.wl_surface());
        let should_render_panel = self.panel.should_render(app)?;

        let popups = PopupManager::popups_for_surface(toplevel.wl_surface())
            .flat_map(|(popup, point)| {
                let configured = with_states(popup.wl_surface(), |states| {
                    states
                        .data_map
                        .get::<XdgPopupSurfaceData>()
                        .unwrap()
                        .lock()
                        .unwrap()
                        .configured
                });

                if !configured {
                    return Vec::new();
                }
                let popup_origin = point - popup.geometry().loc;
                collect_rendered_surface_tree_at(popup.wl_surface(), popup_origin, true)
            })
            .collect::<Vec<_>>();

        let mut tree_dirty = false;

        if let Some(wvr_server) = app.wvr_server.as_mut() {
            let state = &mut wvr_server.manager.state;
            tree_dirty |= state.take_redraw_request(&surface_id);
            tree_dirty |= state.has_pending_frame_callbacks(&surface_id);
            for surface in &surfaces {
                tree_dirty |= state.take_redraw_request(&surface.surface_id);
                tree_dirty |= state.has_pending_frame_callbacks(&surface.surface_id);
            }
            for popup in &popups {
                tree_dirty |= state.take_redraw_request(&popup.surface_id);
                tree_dirty |= state.has_pending_frame_callbacks(&popup.surface_id);
            }
        }

        self.surfaces = surfaces;

        let force_render = tree_dirty || mem::take(&mut self.just_resumed);

        with_states(toplevel.wl_surface(), |states| {
            if let Some(surf) = SurfaceBufWithImage::get_from_surface(states) {
                let mut meta = FrameMeta {
                    extent: surf.image.extent_u32arr(),
                    format: surf.image.format(),
                    clear: WGfxClearMode::Clear([0.0, 0.0, 0.0, 0.0]),
                    stereo: self.stereo.unwrap_or(StereoMode::None),
                    ..Default::default()
                };

                if let Some(stereo) = self.stereo {
                    // Apply stereo full frame logic
                    if self.stereo_full_frame {
                        match stereo {
                            StereoMode::LeftRight | StereoMode::RightLeft => {
                                meta.extent[0] /= 2;
                            }
                            StereoMode::TopBottom | StereoMode::BottomTop => {
                                meta.extent[1] /= 2;
                            }
                            _ => {}
                        }
                    }
                }

                let inner_extent = meta.extent;
                meta.extent[0] += BORDER_SIZE * 2;
                meta.extent[1] += BORDER_SIZE * 2 + BAR_SIZE;

                if let Some(pipeline) = self.pipeline.as_mut() {
                    if self.inner_extent != inner_extent {
                        pipeline.set_layout(
                            app,
                            [inner_extent[0] as _, inner_extent[1] as _],
                            [BORDER_SIZE as _, (BAR_SIZE + BORDER_SIZE) as _],
                            Transform::Normal,
                        )?;
                        self.apply_extent(app, &meta)?;
                        self.inner_extent = inner_extent;
                    }
                } else {
                    let pipeline = ScreenPipeline::new(
                        &meta,
                        app,
                        self.stereo.unwrap_or(StereoMode::None),
                        [BORDER_SIZE as _, (BAR_SIZE + BORDER_SIZE) as _],
                        Transform::Normal,
                    )?;
                    self.apply_extent(app, &meta)?;
                    self.pipeline = Some(pipeline);
                }

                let mouse = app
                    .wvr_server
                    .as_ref()
                    .unwrap()
                    .wm
                    .mouse
                    .as_ref()
                    .filter(|m| m.hover_window == self.window)
                    .map(|m| MouseMeta {
                        x: (m.x as f32) / (inner_extent[0] as f32),
                        y: (m.y as f32) / (inner_extent[1] as f32),
                    });

                let dirty = self.mouse != mouse || rendered_surfaces_dirty(&self.popups, &popups);
                self.mouse = mouse;
                self.popups = popups;
                self.meta = Some(meta);

                if force_render {
                    self.cur_image = Some(surf.image);
                    return Ok(ShouldRender::Should);
                }

                if self
                    .cur_image
                    .as_ref()
                    .is_none_or(|i| *i.image() != *surf.image.image())
                {
                    log::trace!(
                        "{}: new {} image",
                        self.name,
                        if surf.dmabuf { "DMA-buf" } else { "SHM" }
                    );
                    self.cur_image = Some(surf.image);
                    Ok(ShouldRender::Should)
                } else if dirty {
                    Ok(ShouldRender::Should)
                } else {
                    Ok(should_render_panel)
                }
            } else {
                log::trace!("{}: no buffer for wl_surface", self.name);
                Ok(ShouldRender::Unable)
            }
        })
    }

    fn render(
        &mut self,
        app: &mut state::AppState,
        rdr: &mut RenderResources,
    ) -> anyhow::Result<()> {
        self.panel.render(app, rdr)?;
        // `GuiPanel` is not stereo-aware, so just render the same pass twice
        if rdr.cmd_bufs.len() > 1 {
            rdr.cmd_bufs.reverse();
            self.panel.render(app, rdr)?;
            rdr.cmd_bufs.reverse();
        }

        let image = self.cur_image.as_ref().unwrap().clone();
        let mut callback_surfaces = Vec::with_capacity(self.surfaces.len() + self.popups.len());

        self.pipeline
            .as_mut()
            .unwrap()
            .render_screen(image, app, rdr)?;

        for surface in &self.surfaces {
            self.render_subsurface(app, rdr, surface)?;
            callback_surfaces.push(&surface.surface_id);
        }

        for popup in &self.popups {
            self.render_subsurface(app, rdr, popup)?;
            callback_surfaces.push(&popup.surface_id);
        }

        if let Some(mouse) = self.mouse.as_ref() {
            self.pipeline.as_mut().unwrap().render_mouse(mouse, rdr)?;
        }

        // frame callbacks for toplevel + subsurf + popup
        if let Some(wvr_server) = app.wvr_server.as_mut() {
            let state = &mut wvr_server.manager.state;
            if let Some(window) = wvr_server.wm.windows.get(&self.window) {
                let surface_id = window.toplevel.wl_surface().id();
                state.send_frame_callbacks_for_surface_id(&surface_id);
            }
            for surface in &self.surfaces {
                state.send_frame_callbacks_for_surface_id(&surface.surface_id);
            }
            for popup in &self.popups {
                state.send_frame_callbacks_for_surface_id(&popup.surface_id);
            }
        }

        Ok(())
    }

    fn frame_meta(&mut self) -> Option<FrameMeta> {
        self.meta
    }

    fn notify(
        &mut self,
        app: &mut state::AppState,
        event_data: OverlayEventData,
    ) -> anyhow::Result<()> {
        match event_data {
            OverlayEventData::IdAssigned(oid) => {
                let wvr_server = app.wvr_server.as_mut().unwrap(); //never None
                wvr_server.overlay_added(oid, self.window);
            }
            OverlayEventData::WvrCommand(WvrCommand::CloseWindow) => {
                app.wvr_server.as_mut().unwrap().close_window(self.window);
            }
            OverlayEventData::WvrCommand(WvrCommand::KillProcess(signal)) => {
                let wvr_server = app.wvr_server.as_mut().unwrap();
                let Some(p) = wvr_server.wm.windows.get(&self.window) else {
                    return Ok(());
                };
                wvr_server.terminate_process(p.process, signal);
            }
            _ => {}
        }

        Ok(())
    }

    fn on_hover(&mut self, app: &mut state::AppState, hit: &input::PointerHit) -> HoverResult {
        match self.hit_target(hit) {
            Some(WvrHitTarget::Panel(hit2)) => {
                self.panel_hovered = true;
                self.panel.on_hover(app, &hit2)
            }
            Some(WvrHitTarget::Popup {
                surface,
                global_pos,
                surface_origin,
            })
            | Some(WvrHitTarget::Surface {
                surface,
                global_pos,
                surface_origin,
            }) => {
                if self.panel_hovered {
                    self.panel.on_left(app, hit.pointer);
                    self.panel_hovered = false;
                }

                let wvr_server = app.wvr_server.as_mut().unwrap();

                wvr_server.send_mouse_move_to_surface(
                    surface,
                    global_pos,
                    surface_origin,
                    self.window,
                );

                HoverResult {
                    haptics: None,
                    consume: true,
                }
            }
            Some(WvrHitTarget::Toplevel { pos }) => {
                if self.panel_hovered {
                    self.panel.on_left(app, hit.pointer);
                    self.panel_hovered = false;
                }

                let wvr_server = app.wvr_server.as_mut().unwrap();

                wvr_server.send_mouse_move(self.window, pos.x as u32, pos.y as u32);

                HoverResult {
                    haptics: None,
                    consume: true,
                }
            }
            None => HoverResult::default(),
        }
    }

    fn on_left(&mut self, app: &mut state::AppState, pointer: usize) {
        if self.panel_hovered {
            self.panel.on_left(app, pointer);
            self.panel_hovered = false;
        }
    }

    fn on_pointer(&mut self, app: &mut state::AppState, hit: &input::PointerHit, pressed: bool) {
        let Some(index) = Self::mouse_index_from_mode(hit.mode) else {
            return;
        };

        let target = self.hit_target(hit);
        let outside_pos = self.unclamped_client_pos_from_hit(hit);

        // if the press was consumed to dismiss a popup, consume the matching release too.
        if !pressed && self.popup_outside_button == Some(index) {
            self.popup_outside_button = None;

            let click_freeze = app.session.config.click_freeze_time_ms;
            app.wvr_server
                .as_mut()
                .unwrap()
                .send_mouse_button_to_toplevel(
                    click_freeze,
                    self.window,
                    outside_pos,
                    index,
                    false,
                );
            return;
        }

        let popup_grab_active = !self.popups.is_empty()
            && app
                .wvr_server
                .as_ref()
                .is_some_and(|server| server.pointer_is_grabbed());

        let outside_grabbed_popup =
            popup_grab_active && !matches!(&target, Some(WvrHitTarget::Popup { .. }));

        if outside_grabbed_popup {
            if pressed {
                self.popup_outside_button = Some(index);
            }

            let click_freeze = app.session.config.click_freeze_time_ms;
            app.wvr_server
                .as_mut()
                .unwrap()
                .send_mouse_button_to_toplevel(
                    click_freeze,
                    self.window,
                    outside_pos,
                    index,
                    pressed,
                );
            return;
        }

        match target {
            Some(WvrHitTarget::Panel(hit2)) => {
                self.panel_hovered = true;
                self.panel.on_pointer(app, &hit2, pressed);
            }

            Some(WvrHitTarget::Popup {
                surface,
                global_pos,
                surface_origin,
            })
            | Some(WvrHitTarget::Surface {
                surface,
                global_pos,
                surface_origin,
            }) => {
                let wvr_server = app.wvr_server.as_mut().unwrap();

                wvr_server.send_mouse_button_to_surface(
                    surface,
                    global_pos,
                    surface_origin,
                    self.window,
                    index,
                    pressed,
                );
            }

            Some(WvrHitTarget::Toplevel { pos }) => {
                let click_freeze = app.session.config.click_freeze_time_ms;
                let wvr_server = app.wvr_server.as_mut().unwrap();

                // normal toplevel click path, only when no popup grab is active.
                wvr_server.send_mouse_move(self.window, pos.x as u32, pos.y as u32);

                if pressed {
                    wvr_server.send_mouse_down(click_freeze, self.window, index);
                } else {
                    wvr_server.send_mouse_up(index);
                }
            }

            None => {}
        }
    }

    fn on_scroll(&mut self, app: &mut state::AppState, hit: &input::PointerHit, delta: WheelDelta) {
        let target = self.hit_target(hit);

        match target {
            Some(WvrHitTarget::Panel(hit2)) => {
                self.panel.on_scroll(app, &hit2, delta);
                let _ = hit2;
            }

            Some(WvrHitTarget::Popup {
                surface,
                global_pos,
                surface_origin,
            })
            | Some(WvrHitTarget::Surface {
                surface,
                global_pos,
                surface_origin,
            }) => {
                let wvr_server = app.wvr_server.as_mut().unwrap();
                wvr_server.send_mouse_scroll_to_surface(
                    surface,
                    global_pos,
                    surface_origin,
                    self.window,
                    delta,
                );
            }
            Some(WvrHitTarget::Toplevel { pos }) => {
                let wvr_server = app.wvr_server.as_mut().unwrap();
                wvr_server.send_mouse_scroll_to_toplevel(self.window, pos, delta);
            }
            None => {}
        }
    }

    fn get_interaction_transform(&mut self) -> Option<Affine2> {
        self.interaction_transform
    }

    fn get_attrib(&self, attrib: BackendAttrib) -> Option<BackendAttribValue> {
        match attrib {
            BackendAttrib::Stereo => self.stereo.map(BackendAttribValue::Stereo),
            BackendAttrib::Icon => Some(BackendAttribValue::Icon(self.icon.clone())),
            BackendAttrib::StereoFullFrame => {
                Some(BackendAttribValue::StereoFullFrame(self.stereo_full_frame))
            }
            BackendAttrib::StereoAdjustMouse => Some(BackendAttribValue::StereoAdjustMouse(
                self.stereo_adjust_mouse,
            )),
            _ => None,
        }
    }
    fn set_attrib(&mut self, app: &mut AppState, value: BackendAttribValue) -> bool {
        match value {
            BackendAttribValue::Stereo(new) => {
                if let Some(stereo) = self.stereo.as_mut() {
                    log::debug!("{}: stereo: {stereo:?} → {new:?}", self.name);
                    *stereo = new;
                    if let Some(pipeline) = self.pipeline.as_mut() {
                        pipeline.ensure_stereo(new);
                    }
                    true
                } else {
                    false
                }
            }
            BackendAttribValue::StereoFullFrame(new) => {
                self.stereo_full_frame = new;
                true
            }
            BackendAttribValue::StereoAdjustMouse(new) => {
                self.stereo_adjust_mouse = new;
                if let Some(meta) = self.meta.take() {
                    let _ = self.apply_extent(app, &meta);
                    self.meta = Some(meta);
                }
                true
            }
            _ => false,
        }
    }
}

fn rendered_surfaces_dirty(old: &[RenderedSurface], new: &[RenderedSurface]) -> bool {
    if old.len() != new.len() {
        return true;
    }

    old.iter().zip(new).any(|(a, b)| {
        a.surface_id != b.surface_id
            || a.pos != b.pos
            || a.size != b.size
            || *a.image.image() != *b.image.image()
    })
}

fn surface_location(states: &smithay::wayland::compositor::SurfaceData) -> Point<i32, Logical> {
    if states.role == Some(SUBSURFACE_ROLE) {
        let mut guard = states.cached_state.get::<SubsurfaceCachedState>();
        guard.current().location
    } else {
        (0, 0).into()
    }
}

fn collect_rendered_surface_tree_at(
    root: &WlSurface,
    initial_pos: Point<i32, Logical>,
    include_root: bool,
) -> Vec<RenderedSurface> {
    let mut out = Vec::new();
    let root_id = root.id();

    with_surface_tree_upward(
        root,
        initial_pos,
        |_, states, parent_pos| {
            let pos = *parent_pos + surface_location(states);

            // Do not skip even if this surface has no buffer;
            // children may still have buffers.
            TraversalAction::DoChildren(pos)
        },
        |surface, states, parent_pos| {
            if !include_root && surface.id() == root_id {
                return;
            }

            let pos = *parent_pos + surface_location(states);

            if let Some(surf) = SurfaceBufWithImage::get_from_surface(states) {
                let extent = surf.image.extent_f32();
                let scale = surf.scale.max(1) as f32;

                out.push(RenderedSurface {
                    surface: surface.clone(),
                    surface_id: surface.id(),
                    image: surf.image,
                    pos: vec2(pos.x as f32, pos.y as f32),
                    size: vec2(extent[0] / scale, extent[1] / scale),
                });
            }
        },
        |_, _, _| true,
    );

    out
}

fn collect_rendered_surface_tree(root: &WlSurface) -> Vec<RenderedSurface> {
    collect_rendered_surface_tree_at(root, Point::<i32, Logical>::from((0, 0)), false)
}

fn surface_accepts_input(surface: &RenderedSurface, global_pos: Vec2) -> bool {
    let local = global_pos - surface.pos;

    if local.x < 0.0 || local.y < 0.0 || local.x >= surface.size.x || local.y >= surface.size.y {
        return false;
    }

    with_states(&surface.surface, |states| {
        let mut guard = states.cached_state.get::<SurfaceAttributes>();
        let attrs = guard.current();

        match attrs.input_region.as_ref() {
            None => true,
            Some(region) => {
                let point =
                    Point::<i32, Logical>::from((local.x.floor() as i32, local.y.floor() as i32));
                region.contains(point)
            }
        }
    })
}
