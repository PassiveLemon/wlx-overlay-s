use std::{rc::Rc, sync::Arc};
use wgui::{
	assets::AssetPath,
	color::WguiColorName,
	components::button::ComponentButton,
	globals::WguiGlobals,
	i18n::Translation,
	layout::{Layout, WidgetID},
	log::LogErr,
	palette::PALETTES,
	parser::{Fetchable, ParseDocumentParams, TemplateParams},
	task::Tasks,
};
use wlx_common::{
	dash_interface::ConfigChangeKind,
	palette::{list_palette_files, load_custom_palette},
};

use crate::{
	frontend::{FrontendTask, FrontendTasks},
	tab::settings::Task as SettingsTask,
	util::popup_manager::{MountPopupOnceParams, PopupHolder},
	views::{self, ViewTrait, ViewUpdateParams},
};

#[derive(Clone)]
enum Task {
	SelectPalette(String),
	CustomPaletteUrl,
	Restart,
	Cancel,
}

pub struct Params<'a> {
	pub globals: WguiGlobals,
	pub layout: &'a mut Layout,
	pub parent_id: WidgetID,
	pub frontend_tasks: &'a FrontendTasks,
	pub settings_tasks: Tasks<SettingsTask>,
	pub current_palette: Arc<str>,
}

pub struct View {
	tasks: Tasks<Task>,
	frontend_tasks: FrontendTasks,
	globals: WguiGlobals,
	popup_dialog: PopupHolder<views::dialog_box::View>,
	settings_tasks: Tasks<SettingsTask>,
	chosen_palette: Option<Arc<str>>,
}

impl ViewTrait for View {
	fn update(&mut self, par: &mut ViewUpdateParams) -> anyhow::Result<()> {
		self.popup_dialog.update(par)?;

		for task in self.tasks.drain() {
			match task {
				Task::SelectPalette(profile) => {
					self.chosen_palette = Some(profile.into());
					self.show_restart_dialog_box()?;
				}
				Task::Cancel => {
					let close_dialog = self.popup_dialog.get_close_callback(par.layout);
					close_dialog();
				}
				Task::Restart => {
					if let Some(palette) = self.chosen_palette.take() {
						par.general_config.color_palette = palette;
						par.config_change_kind.replace(ConfigChangeKind::WguiThemeChange);
					}

					self.settings_tasks.push(SettingsTask::RestartSoftware);
				}
				Task::CustomPaletteUrl => {
					self.frontend_tasks.push(FrontendTask::OpenURL(
						"https://wayvr.org/docs/basics/customization/".into(),
					));
				}
			}
		}
		Ok(())
	}
}

macro_rules! insert_colors {
	(
		$params:expr,
		$palette:expr,
		$( $key:literal => $color:ident ),* $(,)?
	) => {
		$(
			$params.insert_str(
				$key,
				WguiColorName::$color
					.to_wgui_color()
					.resolve($palette)
					.to_hex()
			);
		)*
	};
}

impl View {
	pub fn new(params: Params) -> anyhow::Result<Self> {
		let doc_params = &ParseDocumentParams {
			globals: params.globals.clone(),
			path: AssetPath::BuiltIn("gui/view/color_palettes.xml"),
			extra: Default::default(),
		};

		let mut parser_state = wgui::parser::parse_from_assets(doc_params, params.layout, params.parent_id)?;

		let list_parent = parser_state.fetch_widget(&params.layout.state, "list_parent")?.id;

		let tasks = Tasks::new();
		let popup_dialog = PopupHolder::<views::dialog_box::View>::default();

		for (idx, name) in list_palette_files().into_iter().enumerate() {
			let Ok(palette) = load_custom_palette(&name).log_warn("Could not load custom color palette") else {
				continue;
			};

			let id = format!("profile_custom_{idx}");
			let is_current = &*params.current_palette == name.as_str();

			let mut cell_params = TemplateParams::new();
			cell_params.insert("id", &id);

			let display_name = &name[..name.len() - 5];

			if is_current {
				cell_params.insert_str("text", format!("{display_name} ✅"));
				cell_params.insert("tooltip", "APP_SETTINGS.COLOR_PALETTE_CURRENT");
			} else {
				cell_params.insert("text", display_name);
				cell_params.insert("tooltip", "APP_SETTINGS.COLOR_PALETTE_ACTIVATE");
			}

			insert_colors!(
				cell_params,
				&palette,
				"primary" => Primary,
				"on_primary" => OnPrimary,
				"secondary" => Secondary,
				"on_secondary" => OnSecondary,
				"tertiary" => Tertiary,
				"on_tertiary" => OnTertiary,
				"danger" => Danger,
				"on_danger" => OnDanger,
				"background" => Background,
				"on_background" => OnBackground,
				"background_variant" => BackgroundVariant,
				"outline" => Outline,
				"highlight" => Highlight,
			);

			parser_state.instantiate_template(
				doc_params,
				"ColorPaletteButton",
				params.layout,
				list_parent,
				cell_params,
			)?;

			if !is_current {
				let btn = parser_state.fetch_component_as::<ComponentButton>(&id)?;
				let tasks_clone = tasks.clone();
				btn.on_click(Rc::new({
					move |_common, _e| {
						tasks_clone.push(Task::SelectPalette(name.to_string()));
						Ok(())
					}
				}));
			}
		}

		for (idx, (name, palette)) in PALETTES.iter().enumerate() {
			let id = format!("profile_builtin_{idx}");
			let is_current = &*params.current_palette == *name;

			let mut cell_params = TemplateParams::new();
			cell_params.insert("id", &id);

			if is_current {
				cell_params.insert_str("text", format!("{name} ✅"));
				cell_params.insert("tooltip", "APP_SETTINGS.COLOR_PALETTE_CURRENT");
			} else {
				cell_params.insert("text", name);
				cell_params.insert("tooltip", "APP_SETTINGS.COLOR_PALETTE_ACTIVATE");
			}

			insert_colors!(
				cell_params,
				palette,
				"primary" => Primary,
				"on_primary" => OnPrimary,
				"secondary" => Secondary,
				"on_secondary" => OnSecondary,
				"tertiary" => Tertiary,
				"on_tertiary" => OnTertiary,
				"danger" => Danger,
				"on_danger" => OnDanger,
				"background" => Background,
				"on_background" => OnBackground,
				"background_variant" => BackgroundVariant,
				"outline" => Outline,
				"highlight" => Highlight,
			);

			parser_state.instantiate_template(
				doc_params,
				"ColorPaletteButton",
				params.layout,
				list_parent,
				cell_params,
			)?;

			if !is_current {
				let btn = parser_state.fetch_component_as::<ComponentButton>(&id)?;
				let tasks_clone = tasks.clone();
				btn.on_click(Rc::new({
					move |_common, _e| {
						tasks_clone.push(Task::SelectPalette(name.to_string()));
						Ok(())
					}
				}));
			}
		}

		parser_state.instantiate_template(
			doc_params,
			"CustomPaletteButton",
			params.layout,
			list_parent,
			TemplateParams::default(),
		)?;
		let btn = parser_state.fetch_component_as::<ComponentButton>("custom_btn")?;
		let tasks_clone = tasks.clone();
		btn.on_click(Rc::new({
			move |_common, _e| {
				tasks_clone.push(Task::CustomPaletteUrl);
				Ok(())
			}
		}));

		Ok(Self {
			tasks,
			frontend_tasks: params.frontend_tasks.clone(),
			globals: params.globals.clone(),
			popup_dialog,
			settings_tasks: params.settings_tasks,
			chosen_palette: None,
		})
	}

	fn show_restart_dialog_box(&mut self) -> anyhow::Result<()> {
		const ACTION_RESTART: &str = "restart";
		const ACTION_CANCEL: &str = "cancel";

		let tasks = self.tasks.clone();

		views::dialog_box::mount_popup(
			self.popup_dialog.clone(),
			self.frontend_tasks.clone(),
			views::dialog_box::Params {
				globals: self.globals.clone(),
				message: Translation::from_translation_key("APP_SETTINGS.APPLY_CHANGES_RESTART"),
				entries: vec![
					views::dialog_box::ButtonEntry {
						content: Translation::from_translation_key("APP_SETTINGS.CANCEL"),
						icon: "dashboard/close.svg",
						action: ACTION_CANCEL,
					},
					views::dialog_box::ButtonEntry {
						content: Translation::from_translation_key("APP_SETTINGS.RESTART_SOFTWARE"),
						icon: "dashboard/refresh.svg",
						action: ACTION_RESTART,
					},
				],
				on_action_click: Box::new(move |action| match action {
					ACTION_RESTART => {
						tasks.push(Task::Restart);
					}
					ACTION_CANCEL => {
						tasks.push(Task::Cancel);
					}
					_ => unreachable!(),
				}),
			},
		);

		Ok(())
	}
}

pub fn mount_popup(
	frontend_tasks: FrontendTasks,
	globals: WguiGlobals,
	popup: PopupHolder<View>,
	settings_tasks: Tasks<SettingsTask>,
	current_palette: Arc<str>,
) {
	frontend_tasks
		.clone()
		.push(FrontendTask::MountPopupOnce(MountPopupOnceParams::new(
			Translation::from_translation_key("APP_SETTINGS.COLOR_PALETTES"),
			Box::new(move |data| {
				let view = View::new(Params {
					globals: globals.clone(),
					layout: data.layout,
					parent_id: data.id_content,
					frontend_tasks: &frontend_tasks,
					settings_tasks,
					current_palette,
				})?;

				popup.set_view(data.handle, view, None);
				Ok(popup.get_close_callback(data.layout))
			}),
			Default::default(), /* extra */
		)));
}
