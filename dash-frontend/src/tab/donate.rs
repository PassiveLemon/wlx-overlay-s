use std::{marker::PhantomData, rc::Rc};

use wgui::{
	assets::AssetPath,
	components::button::ComponentButton,
	globals::WguiGlobals,
	layout::{Layout, WidgetID},
	parser::{Fetchable, ParseDocumentParams, ParserState, TemplateParams},
	task::Tasks,
};

use crate::{
	frontend::{Frontend, FrontendTask, FrontendTasks},
	tab::{Tab, TabType},
};

#[derive(Clone)]
#[allow(clippy::enum_variant_names)]
enum Task {}

pub struct TabDonate<T> {
	#[allow(dead_code)]
	pub state: ParserState,
	marker: PhantomData<T>,
	tasks: Tasks<Task>,
	frontend_tasks: FrontendTasks,
}

impl<T> Tab<T> for TabDonate<T> {
	fn get_type(&self) -> TabType {
		TabType::Donate
	}

	fn update(&mut self, frontend: &mut Frontend<T>, _time_ms: u32, _user_data: &mut T) -> anyhow::Result<()> {
		for task in self.tasks.drain() {
			match task {}
		}

		Ok(())
	}
}

fn doc_params(globals: &WguiGlobals) -> ParseDocumentParams<'_> {
	ParseDocumentParams {
		globals: globals.clone(),
		path: AssetPath::BuiltIn("gui/tab/donate.xml"),
		extra: Default::default(),
	}
}

impl<T> TabDonate<T> {
	pub fn new(frontend: &mut Frontend<T>, parent_id: WidgetID, _data: &mut T) -> anyhow::Result<Self> {
		let state = wgui::parser::parse_from_assets(&doc_params(&frontend.globals), &mut frontend.layout, parent_id)?;

		let tasks = Tasks::<Task>::new();

		Ok(Self {
			state,
			marker: PhantomData,
			tasks,
			frontend_tasks: frontend.tasks.clone(),
		})
	}
}
