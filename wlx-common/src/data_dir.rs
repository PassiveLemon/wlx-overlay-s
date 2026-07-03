use std::{path::PathBuf, sync::LazyLock};

const FALLBACK_DATA_PATH: &str = "/tmp/wayvr_data";

static DATA_ROOT_PATH: LazyLock<PathBuf> = LazyLock::new(|| {
	if let Some(mut dir) = xdg::BaseDirectories::new().get_data_home() {
		dir.push("wayvr");
		return dir;
	}
	log::error!("Err: Failed to find data path, using {FALLBACK_DATA_PATH}");
	PathBuf::from(FALLBACK_DATA_PATH)
});

fn get_data_root() -> PathBuf {
	DATA_ROOT_PATH.clone()
}

pub fn get_path(data_path: &str) -> PathBuf {
	get_data_root().join(data_path)
}
