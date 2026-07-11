use taffy::{
	AlignContent, AlignItems, AlignSelf, BoxSizing, Display, FlexDirection, FlexWrap, JustifyContent, JustifySelf,
	Overflow,
};

use crate::{
	color::WguiColor,
	drawing,
	parser::{AttribPair, ParserContext, is_percent, parse_f32},
	renderer_vk::text::{FontWeight, HorizontalAlign, TextStyle},
	widget::util::WLength,
};

pub fn parse_round(
	ctx: &ParserContext<'_>,
	tag_name: &str,
	key: &str,
	value: &str,
	round: &mut WLength,
	multiplier: f32,
) {
	if is_percent(value) {
		if let Some(val) = ctx.parse_percent(tag_name, key, value) {
			*round = WLength::Percent(val);
		} else {
			ctx.print_invalid_attrib(tag_name, key, value);
		}
	} else if let Some(val) = parse_f32(value) {
		*round = WLength::Units((val * multiplier).max(0.));
	} else {
		ctx.print_invalid_attrib(tag_name, key, value);
	}
}

pub fn parse_color(ctx: &ParserContext<'_>, tag_name: &str, key: &str, value: &str, out_color: &mut WguiColor) -> bool {
	if let Some(color) = drawing::Color::from_hex(value) {
		*out_color = color.into();
		return true;
	}

	if let Some(color) = ctx.layout.state.globals.get().palette.find(value) {
		*out_color = color;
		return true;
	}

	ctx.print_invalid_attrib(tag_name, key, value);
	false
}

pub fn parse_color_opt(
	ctx: &ParserContext<'_>,
	tag_name: &str,
	key: &str,
	value: &str,
	out_color: &mut Option<WguiColor>,
) {
	let mut color = WguiColor::default();
	if parse_color(ctx, tag_name, key, value, &mut color) {
		*out_color = Some(color);
	}
}

pub fn parse_text_style(ctx: &ParserContext<'_>, attribs: &[AttribPair], tag_name: &str, prefix: &str) -> TextStyle {
	let mut style = TextStyle::default();

	for pair in attribs {
		let (key, value) = (pair.attrib.as_ref(), pair.value.as_ref());
		if key.starts_with(prefix) {
			let suffix = &key[prefix.len()..];
			match suffix {
				"color" => {
					parse_color_opt(ctx, tag_name, key, value, &mut style.color);
				}
				"align" => match value {
					"left" => style.align = Some(HorizontalAlign::Left),
					"right" => style.align = Some(HorizontalAlign::Right),
					"center" => style.align = Some(HorizontalAlign::Center),
					"justified" => style.align = Some(HorizontalAlign::Justified),
					"end" => style.align = Some(HorizontalAlign::End),
					_ => {
						ctx.print_invalid_attrib(tag_name, key, value);
					}
				},
				"weight" => match value {
					"light" => style.weight = Some(FontWeight::Light),
					"normal" => style.weight = Some(FontWeight::Normal),
					"bold" => style.weight = Some(FontWeight::Bold),
					_ => {
						ctx.print_invalid_attrib(tag_name, key, value);
					}
				},
				"size" => {
					if let Ok(size) = value.parse::<f32>() {
						style.size = Some(size);
					} else {
						ctx.print_invalid_attrib(tag_name, key, value);
					}
				}
				"shadow" => {
					let mut color = WguiColor::default();
					if parse_color(ctx, tag_name, key, value, &mut color) {
						style.shadow.get_or_insert_default().color = color;
					}
				}
				"shadow_x" => {
					if let Ok(x) = value.parse::<f32>() {
						style.shadow.get_or_insert_default().x = x;
					} else {
						ctx.print_invalid_attrib(tag_name, key, value);
					}
				}
				"shadow_y" => {
					if let Ok(y) = value.parse::<f32>() {
						style.shadow.get_or_insert_default().y = y;
					} else {
						ctx.print_invalid_attrib(tag_name, key, value);
					}
				}
				"wrap" => {
					if let Ok(y) = value.parse::<i32>() {
						style.wrap = y == 1;
					} else {
						ctx.print_invalid_attrib(tag_name, key, value);
					}
				}
				_ => {}
			}
		}
	}

	style
}

#[allow(clippy::cognitive_complexity)]
pub fn parse_style(ctx: &ParserContext<'_>, attribs: &[AttribPair], tag_name: &str) -> taffy::Style {
	let mut style = taffy::Style::default();

	for pair in attribs {
		let (key, value) = (pair.attrib.as_ref(), pair.value.as_ref());
		match key {
			"display" => match value {
				"flex" => style.display = Display::Flex,
				"block" => style.display = Display::Block,
				"grid" => style.display = Display::Grid,
				"none" => style.display = Display::None,
				_ => {
					ctx.print_invalid_attrib(tag_name, key, value);
				}
			},
			"margin_left" => {
				if let Some(dim) = ctx.parse_size_unit(tag_name, key, value) {
					style.margin.left = dim;
				}
			}
			"margin_right" => {
				if let Some(dim) = ctx.parse_size_unit(tag_name, key, value) {
					style.margin.right = dim;
				}
			}
			"margin_top" => {
				if let Some(dim) = ctx.parse_size_unit(tag_name, key, value) {
					style.margin.top = dim;
				}
			}
			"margin_bottom" => {
				if let Some(dim) = ctx.parse_size_unit(tag_name, key, value) {
					style.margin.bottom = dim;
				}
			}
			"padding_left" => {
				if let Some(dim) = ctx.parse_size_unit(tag_name, key, value) {
					style.padding.left = dim;
				}
			}
			"padding_right" => {
				if let Some(dim) = ctx.parse_size_unit(tag_name, key, value) {
					style.padding.right = dim;
				}
			}
			"padding_top" => {
				if let Some(dim) = ctx.parse_size_unit(tag_name, key, value) {
					style.padding.top = dim;
				}
			}
			"padding_bottom" => {
				if let Some(dim) = ctx.parse_size_unit(tag_name, key, value) {
					style.padding.bottom = dim;
				}
			}
			"margin" => {
				if let Some(dim) = ctx.parse_size_unit(tag_name, key, value) {
					style.margin.left = dim;
					style.margin.right = dim;
					style.margin.top = dim;
					style.margin.bottom = dim;
				}
			}
			"padding" => {
				if let Some(dim) = ctx.parse_size_unit(tag_name, key, value) {
					style.padding.left = dim;
					style.padding.right = dim;
					style.padding.top = dim;
					style.padding.bottom = dim;
				}
			}
			"overflow" => match value {
				"hidden" => {
					style.overflow.x = Overflow::Hidden;
					style.overflow.y = Overflow::Hidden;
				}
				"visible" => {
					style.overflow.x = Overflow::Visible;
					style.overflow.y = Overflow::Visible;
				}
				"clip" => {
					style.overflow.x = Overflow::Clip;
					style.overflow.y = Overflow::Clip;
				}
				"scroll" => {
					style.overflow.x = Overflow::Scroll;
					style.overflow.y = Overflow::Scroll;
				}
				_ => {
					ctx.print_invalid_attrib(tag_name, key, value);
				}
			},
			"overflow_x" => match value {
				"hidden" => style.overflow.x = Overflow::Hidden,
				"visible" => style.overflow.x = Overflow::Visible,
				"clip" => style.overflow.x = Overflow::Clip,
				"scroll" => style.overflow.x = Overflow::Scroll,
				_ => {
					ctx.print_invalid_attrib(tag_name, key, value);
				}
			},
			"overflow_y" => match value {
				"hidden" => style.overflow.y = Overflow::Hidden,
				"visible" => style.overflow.y = Overflow::Visible,
				"clip" => style.overflow.y = Overflow::Clip,
				"scroll" => style.overflow.y = Overflow::Scroll,
				_ => {
					ctx.print_invalid_attrib(tag_name, key, value);
				}
			},
			"aspect_ratio" => {
				if let Some(aspect) = ctx.parse_val(tag_name, key, value) {
					style.aspect_ratio = Some(aspect);
				}
			}
			"min_width" => {
				if let Some(dim) = ctx.parse_size_unit(tag_name, key, value) {
					style.min_size.width = dim;
				} else if ParserContext::parse_auto(value) {
					style.min_size.width = taffy::prelude::auto();
				}
			}
			"min_height" => {
				if let Some(dim) = ctx.parse_size_unit(tag_name, key, value) {
					style.min_size.height = dim;
				} else if ParserContext::parse_auto(value) {
					style.min_size.height = taffy::prelude::auto();
				}
			}
			"max_width" => {
				if let Some(dim) = ctx.parse_size_unit(tag_name, key, value) {
					style.max_size.width = dim;
				} else if ParserContext::parse_auto(value) {
					style.max_size.width = taffy::prelude::auto();
				}
			}
			"max_height" => {
				if let Some(dim) = ctx.parse_size_unit(tag_name, key, value) {
					style.max_size.height = dim;
				} else if ParserContext::parse_auto(value) {
					style.max_size.height = taffy::prelude::auto();
				}
			}
			"width" => {
				if let Some(dim) = ctx.parse_size_unit(tag_name, key, value) {
					style.size.width = dim;
				} else if ParserContext::parse_auto(value) {
					style.size.width = taffy::prelude::auto();
				}
			}
			"height" => {
				if let Some(dim) = ctx.parse_size_unit(tag_name, key, value) {
					style.size.height = dim;
				} else if ParserContext::parse_auto(value) {
					style.size.height = taffy::prelude::auto();
				}
			}
			"gap" => {
				if let Some(val) = ctx.parse_size_unit(tag_name, key, value) {
					style.gap = val;
				}
			}
			"flex_basis" => {
				if let Some(val) = ctx.parse_size_unit(tag_name, key, value) {
					style.flex_basis = val;
				}
			}
			"flex_grow" => {
				if let Some(val) = ctx.parse_val(tag_name, key, value) {
					style.flex_grow = val;
				}
			}
			"flex_shrink" => {
				if let Some(val) = ctx.parse_val(tag_name, key, value) {
					style.flex_shrink = val;
				}
			}
			"position" => match value {
				"absolute" => style.position = taffy::Position::Absolute,
				"relative" => style.position = taffy::Position::Relative,
				_ => {
					ctx.print_invalid_attrib(tag_name, key, value);
				}
			},
			"box_sizing" => match value {
				"border_box" => style.box_sizing = BoxSizing::BorderBox,
				"content_box" => style.box_sizing = BoxSizing::ContentBox,
				_ => {
					ctx.print_invalid_attrib(tag_name, key, value);
				}
			},
			"align_self" => match value {
				"baseline" => style.align_self = Some(AlignSelf::BASELINE),
				"center" => style.align_self = Some(AlignSelf::CENTER),
				"end" => style.align_self = Some(AlignSelf::END),
				"flex_end" => style.align_self = Some(AlignSelf::FLEX_END),
				"flex_start" => style.align_self = Some(AlignSelf::FLEX_START),
				"start" => style.align_self = Some(AlignSelf::START),
				"stretch" => style.align_self = Some(AlignSelf::STRETCH),
				_ => {
					ctx.print_invalid_attrib(tag_name, key, value);
				}
			},
			"justify_self" => match value {
				"center" => style.justify_self = Some(JustifySelf::CENTER),
				"end" => style.justify_self = Some(JustifySelf::END),
				"flex_end" => style.justify_self = Some(JustifySelf::FLEX_END),
				"flex_start" => style.justify_self = Some(JustifySelf::FLEX_START),
				"start" => style.justify_self = Some(JustifySelf::START),
				"stretch" => style.justify_self = Some(JustifySelf::STRETCH),
				_ => {
					ctx.print_invalid_attrib(tag_name, key, value);
				}
			},
			"align_items" => match value {
				"baseline" => style.align_items = Some(AlignItems::BASELINE),
				"center" => style.align_items = Some(AlignItems::CENTER),
				"end" => style.align_items = Some(AlignItems::END),
				"flex_end" => style.align_items = Some(AlignItems::FLEX_END),
				"flex_start" => style.align_items = Some(AlignItems::FLEX_START),
				"start" => style.align_items = Some(AlignItems::START),
				"stretch" => style.align_items = Some(AlignItems::STRETCH),
				_ => {
					ctx.print_invalid_attrib(tag_name, key, value);
				}
			},
			"align_content" => match value {
				"center" => style.align_content = Some(AlignContent::CENTER),
				"end" => style.align_content = Some(AlignContent::END),
				"flex_end" => style.align_content = Some(AlignContent::FLEX_END),
				"flex_start" => style.align_content = Some(AlignContent::FLEX_START),
				"space_around" => style.align_content = Some(AlignContent::SPACE_AROUND),
				"space_between" => style.align_content = Some(AlignContent::SPACE_BETWEEN),
				"space_evenly" => style.align_content = Some(AlignContent::SPACE_EVENLY),
				"start" => style.align_content = Some(AlignContent::START),
				"stretch" => style.align_content = Some(AlignContent::STRETCH),
				_ => {
					ctx.print_invalid_attrib(tag_name, key, value);
				}
			},
			"justify_content" => match value {
				"center" => style.justify_content = Some(JustifyContent::CENTER),
				"end" => style.justify_content = Some(JustifyContent::END),
				"flex_end" => style.justify_content = Some(JustifyContent::FLEX_END),
				"flex_start" => style.justify_content = Some(JustifyContent::FLEX_START),
				"space_around" => style.justify_content = Some(JustifyContent::SPACE_AROUND),
				"space_between" => style.justify_content = Some(JustifyContent::SPACE_BETWEEN),
				"space_evenly" => style.justify_content = Some(JustifyContent::SPACE_EVENLY),
				"start" => style.justify_content = Some(JustifyContent::START),
				"stretch" => style.justify_content = Some(JustifyContent::STRETCH),
				_ => {
					ctx.print_invalid_attrib(tag_name, key, value);
				}
			},
			"flex_wrap" => match value {
				"wrap" => style.flex_wrap = FlexWrap::Wrap,
				"no_wrap" => style.flex_wrap = FlexWrap::NoWrap,
				"wrap_reverse" => style.flex_wrap = FlexWrap::WrapReverse,
				_ => {}
			},
			"flex_direction" => match value {
				"column_reverse" => style.flex_direction = FlexDirection::ColumnReverse,
				"column" => style.flex_direction = FlexDirection::Column,
				"row_reverse" => style.flex_direction = FlexDirection::RowReverse,
				"row" => style.flex_direction = FlexDirection::Row,
				_ => {
					ctx.print_invalid_attrib(tag_name, key, value);
				}
			},
			_ => {}
		}
	}

	style
}
