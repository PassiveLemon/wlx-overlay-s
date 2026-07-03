use std::{fs, io, path::PathBuf};

use wlx_common::data_dir;

pub struct WhisperModel {
	pub file_name: &'static str,
	pub display_name: &'static str,
	pub url: &'static str,
	pub hash: &'static str,
}

pub const WHISPER_MODELS: &[WhisperModel] = &[
	WhisperModel {
		file_name: "ggml-base-q8_0.bin",
		display_name: "Base Q8 (78MiB)",
		url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base-q8_0.bin",
		hash: "7bb89bb49ed6955013b166f1b6a6c04584a20fbe",
	},
	WhisperModel {
		file_name: "ggml-small-q8_0.bin",
		display_name: "Small Q8 (252MiB)",
		url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small-q8_0.bin",
		hash: "bcad8a2083f4e53d648d586b7dbc0cd673d8afad",
	},
	WhisperModel {
		file_name: "ggml-large-v3-turbo-q5_0.bin",
		display_name: "Turbo Q5 (574MiB)",
		url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3-turbo-q5_0.bin",
		hash: "e050f7970618a659205450ad97eb95a18d69c9ee",
	},
	WhisperModel {
		file_name: "ggml-large-v3-turbo-q8_0.bin",
		display_name: "Turbo Q8 (874MiB)",
		url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3-turbo-q8_0.bin",
		hash: "01bf15bedffe9f39d65c1b6ff9b687ea91f59e0e",
	},
	WhisperModel {
		file_name: "ggml-large-v3-turbo.bin",
		display_name: "Turbo (1.5GiB)",
		url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3-turbo.bin",
		hash: "4af2b29d7ec73d781377bfd1758ca957a807e941",
	},
];

pub fn whisper_model_from_name(file_name: &str) -> Option<&'static WhisperModel> {
	WHISPER_MODELS.iter().find(|x| x.file_name == file_name)
}

pub fn whisper_model_folder() -> PathBuf {
	data_dir::get_path("whisper")
}

pub fn whisper_model_path(file_name: &str) -> PathBuf {
	whisper_model_folder().join(file_name)
}

pub fn whisper_any_models_downloaded() -> io::Result<bool> {
	let path = whisper_model_folder();
	if !path.is_dir() {
		return Ok(false);
	}
	Ok(fs::read_dir(path)?.count() > 0)
}

pub fn whisper_delete_all_models() -> io::Result<()> {
	let path = whisper_model_folder();
	if !path.is_dir() {
		return Ok(());
	}

	for entry in fs::read_dir(path)? {
		let entry = entry?;
		let file_type = entry.file_type()?;

		if file_type.is_file() || file_type.is_symlink() {
			fs::remove_file(entry.path())?;
		}
	}

	Ok(())
}
