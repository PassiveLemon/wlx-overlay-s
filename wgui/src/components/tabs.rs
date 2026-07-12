use crate::{
	assets::AssetPath,
	color::WguiColorName,
	components::{
		Component, ComponentBase, ComponentTrait, RefreshData,
		button::{self, ComponentButton},
	},
	event::CallbackDataCommon,
	i18n::Translation,
	layout::WidgetPair,
	widget::{ConstructEssentials, div::WidgetDiv},
};
use std::{cell::RefCell, rc::Rc};
use taffy::{
	AlignItems,
	prelude::{length, percent},
};

pub struct Entry<'a> {
	pub sprite_src: Option<AssetPath<'a>>,
	pub text: Translation,
	pub name: &'a str,
}

pub struct Params<'a> {
	pub style: taffy::Style,
	pub entries: Vec<Entry<'a>>,
	pub selected_entry_name: &'a str, // default: ""
	pub on_select: Option<TabSelectCallback>,
}

struct MountedEntry {
	name: Rc<str>,
	button: Rc<ComponentButton>,
}

pub struct TabSelectEvent {
	pub name: Rc<str>,
}

pub type TabSelectCallback = Rc<dyn Fn(&mut CallbackDataCommon, TabSelectEvent) -> anyhow::Result<()>>;

struct State {
	mounted_entries: Vec<MountedEntry>,
	selected_entry_name: Rc<str>,
	on_select: Option<TabSelectCallback>,
}

pub struct ComponentTabs {
	base: ComponentBase,
	state: Rc<RefCell<State>>,
}

impl ComponentTrait for ComponentTabs {
	fn base(&self) -> &ComponentBase {
		&self.base
	}

	fn base_mut(&mut self) -> &mut ComponentBase {
		&mut self.base
	}

	fn refresh(&self, _data: &mut RefreshData) {
		// nothing to do
	}
}

fn set_button_selected(common: &mut CallbackDataCommon, button: &Rc<ComponentButton>, selected: bool) {
	if selected {
		button.set_color(common, WguiColorName::Primary.into());
	} else {
		button.set_color(common, WguiColorName::Background.into());
	}
}

impl State {
	fn select_entry(&mut self, common: &mut CallbackDataCommon, name: &Rc<str>) {
		for entry in &self.mounted_entries {
			set_button_selected(common, &entry.button, *entry.name == **name);
		}
		self.selected_entry_name = name.clone();

		if let Some(on_select) = self.on_select.clone() {
			let evt = TabSelectEvent { name: name.clone() };
			common.alterables.dispatch(Box::new(move |common| {
				(*on_select)(common, evt)?;
				Ok(())
			}));
		}
	}
}

impl ComponentTabs {
	pub fn on_select(&self, callback: TabSelectCallback) {
		self.state.borrow_mut().on_select = Some(callback);
	}

	pub fn get_tab_button(&self, name: &str) -> Option<Rc<ComponentButton>> {
		self
			.state
			.borrow_mut()
			.mounted_entries
			.iter()
			.find(|e| name == &*e.name)
			.map(|e| e.button.clone())
	}
}

pub fn construct(ess: &mut ConstructEssentials, params: Params) -> anyhow::Result<(WidgetPair, Rc<ComponentTabs>)> {
	let mut style = params.style;

	// force-override style
	style.overflow.y = taffy::Overflow::Scroll;
	style.flex_direction = taffy::FlexDirection::Column;
	style.flex_wrap = taffy::FlexWrap::NoWrap;
	style.align_items = Some(AlignItems::CENTER);
	style.gap = length(4.0);

	let (root, _) = ess.layout.add_child(ess.parent, WidgetDiv::create(), style)?;

	let mut mounted_entries = Vec::<MountedEntry>::new();

	// Mount entries
	for entry in params.entries {
		let (_, button) = button::construct(
			&mut ConstructEssentials {
				layout: ess.layout,
				parent: root.id,
			},
			button::Params {
				text: Some(entry.text),
				sprite_src: entry.sprite_src,
				style: taffy::Style {
					min_size: taffy::Size {
						width: percent(1.0),
						height: length(32.0),
					},
					justify_content: Some(taffy::JustifyContent::START),
					..Default::default()
				},
				..Default::default()
			},
		)?;

		// init colors
		set_button_selected(&mut ess.layout.common(), &button, false);

		mounted_entries.push(MountedEntry {
			name: Rc::from(entry.name),
			button,
		});
	}

	let state = Rc::new(RefCell::new(State {
		selected_entry_name: Rc::from(params.selected_entry_name),
		mounted_entries,
		on_select: params.on_select,
	}));

	// handle button clicks
	for entry in &state.borrow().mounted_entries {
		entry.button.on_click({
			let entry_name = entry.name.clone();
			let state = state.clone();
			Rc::new(move |common, _| {
				state.borrow_mut().select_entry(common, &entry_name);
				Ok(())
			})
		});
	}

	let base = ComponentBase {
		id: root.id,
		lhandles: Default::default(),
	};

	let tabs = Rc::new(ComponentTabs { base, state });

	ess.layout.defer_component_refresh(Component(tabs.clone()));
	Ok((root, tabs))
}
