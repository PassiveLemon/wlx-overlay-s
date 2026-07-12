use std::rc::Rc;

use wgui::{
	components::button::{ButtonClickEvent, ComponentButton},
	i18n::Translation,
	layout::WidgetID,
	palette::PALETTES,
	parser::{Fetchable, TemplateParams},
	widget::label::WidgetLabel,
	windowing::context_menu,
};
use wlx_common::{config::GeneralConfig, dash_interface::ConfigChangeKind};

use crate::tab::settings::{
	SettingType, SettingsMountParams, SettingsTab, Task, horiz_cell,
	macros::{MacroParams, options_category, options_checkbox, options_dropdown, options_slider_f32},
};

pub struct State {}

impl SettingsTab for State {
	fn context_menu_custom(
		&mut self,
		action: Rc<str>,
		config: &mut GeneralConfig,
		change_kind: &mut Option<ConfigChangeKind>,
		layout: &mut wgui::layout::Layout,
		state: &mut wgui::parser::ParserState,
	) -> anyhow::Result<()> {
		if let (Some(_), Some(id), Some(palette_name)) = {
			let mut s = action.splitn(3, ';');
			(s.next(), s.next(), s.next())
		} {
			let mut common = layout.common();
			let mut label = state.fetch_widget_as::<WidgetLabel>(common.state, &format!("{id}_value"))?;
			label.set_text(&mut common, Translation::from_raw_text(palette_name));

			config.color_palette = palette_name.into();
			change_kind.replace(ConfigChangeKind::WguiThemeChange);
		}

		Ok(())
	}
}

impl State {
	pub fn mount(par: SettingsMountParams) -> anyhow::Result<State> {
		let c = options_category(
			par.mp,
			par.id_parent,
			"APP_SETTINGS.LOOK_AND_FEEL",
			"dashboard/palette.svg",
		)?;
		options_dropdown::<wlx_common::locale::Language>(par.mp, c, &SettingType::Language)?;
		palettes_dropdown(par.mp, c)?;
		options_checkbox(par.mp, c, SettingType::OpaqueBackground)?;
		options_checkbox(par.mp, c, SettingType::HideUsername)?;
		options_checkbox(par.mp, c, SettingType::HideGrabHelp)?;
		options_slider_f32(par.mp, c, SettingType::DefaultOverlayScale, 0.7, 1.5, 0.05)?; // min, max, step
		options_slider_f32(par.mp, c, SettingType::UiAnimationSpeed, 0.5, 5.0, 0.1)?; // min, max, step
		options_slider_f32(par.mp, c, SettingType::UiGradientIntensity, 0.0, 1.0, 0.05)?; // min, max, step
		options_slider_f32(par.mp, c, SettingType::UiRoundMultiplier, 0.1, 5.0, 0.1)?;
		options_checkbox(par.mp, c, SettingType::EnableWatch)?;
		options_checkbox(par.mp, c, SettingType::SetsOnWatch)?;
		options_checkbox(par.mp, c, SettingType::Clock12h)?;
		Ok(State {})
	}
}

fn palettes_dropdown(mp: &mut MacroParams, parent: WidgetID) -> anyhow::Result<()> {
	let id = mp.idx.to_string();
	mp.idx += 1;

	let parent = horiz_cell(mp.layout, parent)?;

	let current_palette = mp.config.color_palette.as_ref();

	let mut params = TemplateParams::new();
	params.insert("id", &id);
	params.insert("translation", "APP_SETTINGS.COLOR_PALETTE".into());

	mp.parser_state
		.instantiate_template(mp.doc_params, "DropdownButton", mp.layout, parent, params)?;

	{
		let mut label = mp
			.parser_state
			.fetch_widget_as::<WidgetLabel>(&mp.layout.state, &format!("{id}_value"))?;
		label.set_text_simple(
			&mut mp.layout.state.globals.get(),
			Translation::from_raw_text(current_palette),
		);
	}

	let btn = mp.parser_state.fetch_component_as::<ComponentButton>(&id)?;
	btn.on_click(Rc::new({
		let parent_tasks = mp.tasks.clone();
		move |_common, e: ButtonClickEvent| {
			let cells = PALETTES //TODO: enumerate custom palettes
				.iter()
				.enumerate()
				.map(|(_, item)| context_menu::Cell {
					action_name: Some(format!(";{id};{}", item.0).into()),
					title: Translation::from_raw_text(item.0),
					tooltip: None,
					attribs: vec![],
				})
				.collect::<Vec<_>>();

			parent_tasks.push(Task::OpenContextMenu(e.mouse_pos_absolute.unwrap_or_default(), cells));
			Ok(())
		}
	}));

	Ok(())
}
