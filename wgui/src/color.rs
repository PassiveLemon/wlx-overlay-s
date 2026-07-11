use crate::drawing;
use num_enum::TryFromPrimitive;
use strum::EnumCount;
// Primary: button color
// OnPrimary: text color placed on the Primary-colored button

#[derive(Debug, Copy, Clone, TryFromPrimitive, EnumCount)]
#[repr(u8)]
pub enum WguiColorName {
	Primary,
	OnPrimary,
	Secondary,
	OnSecondary,
	Tertiary,
	OnTertiary,
	Danger,
	OnDanger,
	Background,
	OnBackground,
	BackgroundVariant,
	OnBackgroundVariant,
	BackgroundContrast,
	OnBackgroundContrast,
	Outline,
	Shadow,
	Highlight,
}

pub struct WguiColorPalette {
	colors: Vec<(drawing::Color, &'static str)>,
}

#[derive(Clone, Copy, Debug)]
pub struct WguiNamedColor {
	name: WguiColorName,
	rgb_multiplier: f32,
	rgb_addition: f32,
	alpha: f32,
}

#[derive(Clone, Copy, Debug)]
pub enum WguiColor {
	Raw(drawing::Color),
	Named(WguiNamedColor),
}

impl WguiColorPalette {
	pub fn new() -> WguiColorPalette {
		let mut colors = Vec::<(drawing::Color, &'static str)>::new();
		colors.resize(WguiColorName::COUNT, Default::default());

		// default theme
		colors[WguiColorName::Primary as usize] = (drawing::Color::from_hex("#21adff").unwrap(), "primary");
		colors[WguiColorName::OnPrimary as usize] = (drawing::Color::from_hex("#eaf7ff").unwrap(), "on_primary");
		colors[WguiColorName::Secondary as usize] = (drawing::Color::from_hex("#424b56").unwrap(), "secondary");
		colors[WguiColorName::OnSecondary as usize] = (drawing::Color::from_hex("#d2e6ff").unwrap(), "on_secondary");
		colors[WguiColorName::Tertiary as usize] = (drawing::Color::from_hex("#10d0b3").unwrap(), "tertiary");
		colors[WguiColorName::OnTertiary as usize] = (drawing::Color::from_hex("#d1fff8").unwrap(), "on_tertiary");
		colors[WguiColorName::Danger as usize] = (drawing::Color::from_hex("#f7469a").unwrap(), "danger");
		colors[WguiColorName::OnDanger as usize] = (drawing::Color::from_hex("#ffebf5").unwrap(), "on_danger");
		colors[WguiColorName::Background as usize] = (drawing::Color::from_hex("#002e43").unwrap(), "background");
		colors[WguiColorName::OnBackground as usize] = (drawing::Color::from_hex("#e4f5f6").unwrap(), "on_background");
		colors[WguiColorName::BackgroundVariant as usize] =
			(drawing::Color::from_hex("#0c5170").unwrap(), "background_variant");
		colors[WguiColorName::OnBackgroundVariant as usize] =
			(drawing::Color::from_hex("#e2fdff").unwrap(), "on_background_variant");
		colors[WguiColorName::BackgroundContrast as usize] =
			(drawing::Color::from_hex("#00131c").unwrap(), "background_contrast");
		colors[WguiColorName::OnBackgroundContrast as usize] =
			(drawing::Color::from_hex("#e4edf6").unwrap(), "on_background_contrast");
		colors[WguiColorName::Outline as usize] = (drawing::Color::from_hex("#1c6788").unwrap(), "outline");
		colors[WguiColorName::Shadow as usize] = (drawing::Color::from_hex("#000000").unwrap(), "shadow");
		colors[WguiColorName::Highlight as usize] = (drawing::Color::from_hex("#ffffff").unwrap(), "highlight");

		WguiColorPalette { colors }
	}

	fn resolve_name(&self, in_name: &str) -> Option<WguiColorName> {
		for (idx, (_, name)) in self.colors.iter().enumerate() {
			if in_name == *name {
				return WguiColorName::try_from(idx as u8).ok();
			}
		}
		None
	}

	fn apply_modifier(color: WguiColor, modifier: &str) -> Option<WguiColor> {
		match modifier {
			"transparent" => Some(color.with_alpha(0.0)),
			"opaque" => Some(color.with_alpha(1.0)),
			other => {
				let (prefix, val_str) = other.rsplit_once('-')?;
				let val = val_str.parse::<f32>().ok()?;
				match prefix {
					"opacity" => Some(color.with_alpha(val)),
					"rgb-mult" => Some(color.mult_rgb(val)),
					"rgb-add" => Some(color.add_rgb(val)),
					_ => None,
				}
			}
		}
	}

	pub fn find(&self, in_name: &str) -> Option<WguiColor> {
		if let Some((base, modifiers_str)) = in_name.split_once('(') {
			let base_name = base.trim();
			let modifiers = modifiers_str.trim_end_matches(')');

			let name = self.resolve_name(base_name)?;
			let mut color = name.to_wgui_color();

			for mod_str in modifiers.split(',') {
				color = WguiColorPalette::apply_modifier(color, mod_str.trim())?;
			}

			Some(color)
		} else {
			self.resolve_name(in_name).map(|n| n.to_wgui_color())
		}
	}
}

impl Default for WguiColorPalette {
	fn default() -> Self {
		Self::new()
	}
}

impl WguiColor {
	pub fn resolve(&self, palette: &WguiColorPalette) -> drawing::Color {
		match &self {
			WguiColor::Raw(color) => *color,
			WguiColor::Named(color) => color.resolve(palette),
		}
	}

	#[must_use]
	pub const fn mult_rgb(&self, mult: f32) -> WguiColor {
		match self {
			WguiColor::Raw(color) => WguiColor::Raw(color.mult_rgb(mult)),
			WguiColor::Named(color) => WguiColor::Named(WguiNamedColor {
				name: color.name,
				rgb_multiplier: color.rgb_multiplier * mult,
				rgb_addition: color.rgb_addition,
				alpha: color.alpha,
			}),
		}
	}

	#[must_use]
	pub const fn add_rgb(&self, addition: f32) -> WguiColor {
		match self {
			WguiColor::Raw(color) => WguiColor::Raw(color.add_rgb(addition)),
			WguiColor::Named(color) => WguiColor::Named(WguiNamedColor {
				name: color.name,
				rgb_multiplier: color.rgb_multiplier,
				rgb_addition: color.rgb_addition + addition,
				alpha: color.alpha,
			}),
		}
	}

	#[must_use]
	pub const fn add_alpha(&self, alpha: f32) -> WguiColor {
		match self {
			WguiColor::Raw(color) => WguiColor::Raw(drawing::Color {
				r: color.r,
				g: color.g,
				b: color.b,
				a: color.a + alpha,
			}),
			WguiColor::Named(color) => WguiColor::Named(WguiNamedColor {
				name: color.name,
				rgb_multiplier: color.rgb_multiplier,
				rgb_addition: color.rgb_addition,
				alpha: color.alpha + alpha,
			}),
		}
	}

	#[must_use]
	pub const fn with_alpha(&self, alpha: f32) -> WguiColor {
		match self {
			WguiColor::Raw(color) => WguiColor::Raw(color.with_alpha(alpha)),
			WguiColor::Named(color) => WguiColor::Named(WguiNamedColor {
				name: color.name,
				rgb_multiplier: color.rgb_multiplier,
				rgb_addition: color.rgb_addition,
				alpha,
			}),
		}
	}

	// returns named colors if val is 0 or 1, raw otherwise
	#[must_use]
	pub fn lerp(&self, palette: &WguiColorPalette, other: &WguiColor, val: f32) -> WguiColor {
		if val <= 0.0 {
			*self
		} else if val >= 1.0 {
			*other
		} else {
			let c1 = self.resolve(palette);
			let c2 = other.resolve(palette);
			c1.lerp(&c2, val).into()
		}
	}
}

impl WguiColorName {
	pub const fn to_wgui_color(&self) -> WguiColor {
		WguiColor::Named(WguiNamedColor {
			name: *self,
			alpha: 1.0,
			rgb_multiplier: 1.0,
			rgb_addition: 0.0,
		})
	}
}

impl From<WguiColorName> for WguiColor {
	fn from(name: WguiColorName) -> Self {
		name.to_wgui_color()
	}
}

impl From<drawing::Color> for WguiColor {
	fn from(color: drawing::Color) -> Self {
		WguiColor::Raw(color)
	}
}

impl Default for WguiColor {
	fn default() -> Self {
		Self::Raw(Default::default())
	}
}

impl WguiNamedColor {
	pub fn resolve(&self, palette: &WguiColorPalette) -> drawing::Color {
		let idx = self.name as usize;
		if idx >= palette.colors.len() {
			// unlikely
			debug_assert!(false);
			return drawing::Color::new(1.0, 0.0, 1.0, 1.0); // Magenta
		}

		let (mut color, _) = palette.colors[idx];
		if self.alpha != 1.0 {
			color.a *= self.alpha;
		}

		if self.rgb_multiplier != 1.0 {
			color.r *= self.rgb_multiplier;
			color.g *= self.rgb_multiplier;
			color.b *= self.rgb_multiplier;
		}

		if self.rgb_addition != 0.0 {
			color.r += self.rgb_addition;
			color.g += self.rgb_addition;
			color.b += self.rgb_addition;
		}

		color
	}
}
