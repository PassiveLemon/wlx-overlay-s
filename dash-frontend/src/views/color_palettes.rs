use std::rc::Rc;
use wgui::{
	assets::AssetPath,
	color::WguiColorName,
	components::button::ComponentButton,
	globals::WguiGlobals,
	i18n::Translation,
	layout::{Layout, WidgetID},
	palette::PALETTES,
	parser::{Fetchable, ParseDocumentParams, TemplateParams},
	task::Tasks,
};
use wlx_common::dash_interface::ConfigChangeKind;

use crate::{
	frontend::{FrontendTask, FrontendTasks},
	tab::settings::Task as SettingsTask,
	util::popup_manager::{MountPopupOnceParams, PopupHolder},
	views::{self, ViewTrait, ViewUpdateParams},
};

#[derive(Clone)]
enum Task {
	SelectPalette(String),
	Restart,
	Cancel,
}

pub struct Params<'a> {
	pub globals: WguiGlobals,
	pub layout: &'a mut Layout,
	pub parent_id: WidgetID,
	pub frontend_tasks: &'a FrontendTasks,
	pub settings_tasks: Tasks<SettingsTask>,
}

pub struct View {
	tasks: Tasks<Task>,
	frontend_tasks: FrontendTasks,
	globals: WguiGlobals,
	popup_dialog: PopupHolder<views::dialog_box::View>,
	settings_tasks: Tasks<SettingsTask>,
}

impl ViewTrait for View {
	fn update(&mut self, par: &mut ViewUpdateParams) -> anyhow::Result<()> {
		self.popup_dialog.update(par)?;

		for task in self.tasks.drain() {
			match task {
				Task::SelectPalette(profile) => {
					par.general_config.color_palette = profile.into();
					par.config_change_kind.replace(ConfigChangeKind::WguiThemeChange);

					self.show_restart_dialog_box()?;
				}
				Task::Cancel => {
					let close_dialog = self.popup_dialog.get_close_callback(par.layout);
					close_dialog();
				}
				Task::Restart => {
					self.settings_tasks.push(SettingsTask::RestartSoftware);
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
			$params.insert(
				$key,
				WguiColorName::$color
					.to_wgui_color()
					.resolve($palette)
					.to_hex()
					.as_str()
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

		for (idx, (name, palette)) in PALETTES.iter().enumerate() {
			let id = format!("profile_btn_{idx}");

			let mut cell_params = TemplateParams::new();
			cell_params.insert("id", &id);
			cell_params.insert("text", name);
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

			let btn = parser_state.fetch_component_as::<ComponentButton>(&id)?;
			let tasks_clone = tasks.clone();
			btn.on_click(Rc::new({
				move |_common, _e| {
					tasks_clone.push(Task::SelectPalette(name.to_string()));
					Ok(())
				}
			}));
		}

		Ok(Self {
			tasks,
			frontend_tasks: params.frontend_tasks.clone(),
			globals: params.globals.clone(),
			popup_dialog,
			settings_tasks: params.settings_tasks,
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
				})?;

				popup.set_view(data.handle, view, None);
				Ok(popup.get_close_callback(data.layout))
			}),
			Default::default(), /* extra */
		)));
}
