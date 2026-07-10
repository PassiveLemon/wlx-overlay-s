#[derive(Clone)]
pub struct WguiTheme {
	pub animation_mult: f32,
	pub rounding_mult: f32,
	pub gradient_intensity: f32, // currently used for buttons
}

impl Default for WguiTheme {
	fn default() -> Self {
		Self {
			animation_mult: 1.0,
			rounding_mult: 1.0,
			gradient_intensity: 0.3,
		}
	}
}
