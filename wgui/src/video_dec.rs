use std::ptr::null_mut;

use bytes::{Buf, Bytes};
use dav1d_sys::{
	DAV1D_ERR_AGAIN, Dav1dContext, Dav1dData, Dav1dPicture, Dav1dSettings, dav1d_close, dav1d_default_settings,
	dav1d_get_picture, dav1d_open, dav1d_picture_unref, dav1d_send_data,
};

pub struct IvfReader {
	file_data: Vec<u8>, // Used by `reader`, do not remove!
	reader: Bytes,
	header_size: usize,
	pub cur_frame: u32,
	pub num_frames: u32,
	pub framerate: f32,
	pub width: u16,
	pub height: u16,
}

// Duck IVF video container. Documentation:
// https://wiki.multimedia.cx/index.php/Duck_IVF
// Yes, it's just as simple as that.
//
// Header:
// bytes 0-3    signature: 'DKIF'
// bytes 4-5    version (should be 0)
// bytes 6-7    length of header in bytes
// bytes 8-11   codec FourCC (e.g., 'VP80')
// bytes 12-13  width in pixels
// bytes 14-15  height in pixels
// bytes 16-19  time base denominator
// bytes 20-23  time base numerator
// bytes 24-27  number of `Frame`s in file
// bytes 28-31  unused
//
// Frame:
//   bytes 0-3    size of frame in bytes (not including the 12-byte header)
//   bytes 4-11   64-bit presentation timestamp
//   bytes 12..   frame data

const IVF_MAGIC: [u8; 4] = [0x44, 0x4B, 0x49, 0x46];

pub enum IvfReadFrameResult<'a> {
	Ok((&'a [u8], u64 /* pts */)),
	EndOfFile,
}

impl IvfReader {
	pub fn new(file_data: Vec<u8>) -> anyhow::Result<IvfReader> {
		// safety: both reader and file_data are located in the same struct
		// file_data won't move at all.
		let mut reader = Bytes::from_static(unsafe { std::mem::transmute::<&[u8], &'static [u8]>(&file_data) });

		// read ivf magic
		let mut header_magic: [u8; 4] = [0; 4];
		reader.try_copy_to_slice(&mut header_magic)?;

		if header_magic != IVF_MAGIC {
			anyhow::bail!("invalid magic");
		}

		let header_version = reader.try_get_u16_le()?;

		if header_version != 0 {
			anyhow::bail!("unsupported version"); // there's no other version than 0
		}

		let header_len = reader.try_get_u16_le()?;
		if header_len != 32 {
			anyhow::bail!("header length mismatching");
		}

		let mut header_fourcc: [u8; 4] = [0; 4];
		reader.try_copy_to_slice(&mut header_fourcc)?;

		let header_width = reader.try_get_u16_le()?;
		let header_height = reader.try_get_u16_le()?;
		let header_timebase_den = reader.try_get_u32_le()?;
		let header_timebase_num = reader.try_get_u32_le()?;
		let header_num_frames = reader.try_get_u32_le()?;

		let framerate = header_timebase_den as f32 / header_timebase_num as f32;

		let mut padding: [u8; 4] = [0; 4];
		reader.try_copy_to_slice(&mut padding)?;

		log::debug!("IvfReader: width {header_width}, height {header_height}, framerate {framerate}");

		let header_size = file_data.len() - reader.remaining();

		Ok(IvfReader {
			header_size,
			file_data,
			reader,
			cur_frame: 0,
			width: header_width,
			height: header_height,
			framerate,
			num_frames: header_num_frames,
		})
	}

	pub fn rewind(&mut self) {
		self.reader = Bytes::from_static(unsafe { std::mem::transmute::<&[u8], &'static [u8]>(&self.file_data) });
		// do not read header again
		self.reader.advance(self.header_size);
	}

	// Read demuxed video packet to a chunk
	pub fn read_frame(&mut self) -> anyhow::Result<IvfReadFrameResult<'_>> {
		if self.reader.remaining() == 0 {
			return Ok(IvfReadFrameResult::EndOfFile);
		}

		let frame_size = self.reader.try_get_u32_le()? as usize;
		let frame_pts = self.reader.try_get_u64_le()?;

		if frame_size > 8 * 1024 * 1024 {
			// something went really wrong
			anyhow::bail!("Invalid frame size {frame_size}");
		}

		// SAFETY: reader.chunk() slice lifetime is the same as Self, no risk here.
		let chunk_a /* 'a */ = unsafe /* it's safe */ {
			let Some(chunk) = self.reader.chunk().get(0..(frame_size)) else {
				anyhow::bail!("chunk read error");
			};
			std::mem::transmute::<&[u8], &'static [u8]>(chunk)
		};

		self.reader.advance(frame_size);

		self.cur_frame += 1;

		Ok(IvfReadFrameResult::Ok((chunk_a, frame_pts)))
	}
}

// RGB with 255 A
pub struct RgbxFrame {
	pub width: u16,
	pub height: u16,
	pub data: Vec<u8>,
}

fn yuv_to_rgb(y: u8, u: u8, v: u8) -> (u8, u8, u8) {
	let y_i = i32::from(y);
	let u_i = i32::from(u) - 128;
	let v_i = i32::from(v) - 128;
	let c = (1192 * (y_i - 16)).max(0);
	let out_r = (c + 1634 * v_i) >> 10;
	let out_g = (c - 401 * u_i - 834 * v_i) >> 10;
	let out_b = (c + 2066 * u_i) >> 10;
	(
		out_r.clamp(0, 255) as u8,
		out_g.clamp(0, 255) as u8,
		out_b.clamp(0, 255) as u8,
	)
}

impl RgbxFrame {
	// Simple, temporary, best-effort yuv420->rgb converter. Not performant at all, but at least it works.
	// Temporary till we get native YUV support in shaders (as 3 vulkan textures).
	// SAFETY: this function expects YUV420 data only.
	#[allow(clippy::uninit_vec)]
	fn from_yuv(
		width: u16,
		height: u16,
		data_y: *const u8,
		data_u: *const u8,
		data_v: *const u8,
		stride_luma: usize,
		stride_chroma: usize,
	) -> RgbxFrame {
		unsafe {
			let channels = 4; // rgbx

			let rgbx_size = width as usize * height as usize * channels;
			let mut rgbx = Vec::<u8>::with_capacity(rgbx_size);
			rgbx.set_len(rgbx_size);

			let ptr_rgbx = rgbx.as_mut_ptr();

			for y in 0..height as usize {
				for x in 0..width as usize {
					let val_y = *data_y.add(y * stride_luma + x);
					let val_u = *data_u.add((y / 2) * stride_chroma + (x / 2));
					let val_v = *data_v.add((y / 2) * stride_chroma + (x / 2));
					let (r, g, b) = yuv_to_rgb(val_y, val_u, val_v);

					let pos = y * (width as usize * channels) + x * channels;
					*ptr_rgbx.add(pos) = r;
					*ptr_rgbx.add(pos + 1) = g;
					*ptr_rgbx.add(pos + 2) = b;
					*ptr_rgbx.add(pos + 3) = 255;
				}
			}

			RgbxFrame {
				width,
				height,
				data: rgbx,
			}
		}
	}
}

pub struct Av1Decoder {
	ctx: *mut Dav1dContext,
}

impl Drop for Av1Decoder {
	fn drop(&mut self) {
		unsafe {
			dav1d_close(&raw mut self.ctx);
		}
	}
}

enum GetPictureResult {
	Ok,
	Again,
	Failed(i32),
}

enum ReadFrameIterResult {
	Ok(RgbxFrame),
	Again,
	EndOfFile,
}

pub enum ReadFrameResult {
	Ok(RgbxFrame),
	EndOfFile,
}

impl Av1Decoder {
	pub fn new() -> anyhow::Result<Self> {
		unsafe {
			let mut set: Dav1dSettings = std::mem::zeroed();
			dav1d_default_settings(&raw mut set);

			let mut ctx = null_mut();
			let ret = dav1d_open(&raw mut ctx, &raw mut set);
			if ret < 0 {
				anyhow::bail!("dav1d_open failed: {ret}");
			}
			Ok(Self { ctx })
		}
	}

	fn get_picture(&mut self, picture: &mut Dav1dPicture) -> GetPictureResult {
		let ret = unsafe { dav1d_get_picture(self.ctx, picture) };
		if ret == DAV1D_ERR_AGAIN {
			return GetPictureResult::Again;
		}

		if ret < 0 {
			return GetPictureResult::Failed(ret);
		}

		GetPictureResult::Ok
	}

	fn send_data(&mut self, reader: &mut IvfReader) -> anyhow::Result<bool> {
		unsafe {
			let (packet, pts) = match reader.read_frame()? {
				IvfReadFrameResult::Ok((packet, pts)) => (packet, pts),
				IvfReadFrameResult::EndOfFile => return Ok(false),
			};
			let mut data: Dav1dData = std::mem::zeroed();
			data.data = packet.as_ptr();
			data.sz = packet.len();
			data.m.timestamp = pts as i64;
			data.m.size = packet.len();
			data.m.offset = -1;
			data.m.duration = 0;

			let ret = dav1d_send_data(self.ctx, &raw mut data);
			if ret < 0 {
				anyhow::bail!("dav1d_send_data failed: {ret}");
			}

			Ok(true)
		}
	}

	fn read_frame_iter(&mut self, reader: &mut IvfReader) -> anyhow::Result<ReadFrameIterResult> {
		unsafe {
			let mut picture: Dav1dPicture = std::mem::zeroed();
			match self.get_picture(&mut picture) {
				GetPictureResult::Ok => {}
				GetPictureResult::Again => {
					dav1d_picture_unref(&raw mut picture);
					if !self.send_data(reader)? {
						return Ok(ReadFrameIterResult::EndOfFile);
					}
					return Ok(ReadFrameIterResult::Again);
				}
				GetPictureResult::Failed(code) => {
					dav1d_picture_unref(&raw mut picture);
					anyhow::bail!("dav1d_get_picture failed: {code}");
				}
			}

			let pic_y = picture.data[0]; // luma Y
			let pic_u = picture.data[1]; // chroma U
			let pic_v = picture.data[2]; // chroma V
			let stride_luma = picture.stride[0];
			let stride_chroma = picture.stride[1];

			let rgbx = RgbxFrame::from_yuv(
				reader.width,
				reader.height,
				pic_y as *const u8,
				pic_u as *const u8,
				pic_v as *const u8,
				stride_luma as usize,
				stride_chroma as usize,
			);

			dav1d_picture_unref(&raw mut picture);

			Ok(ReadFrameIterResult::Ok(rgbx))
		}
	}

	pub fn read_frame(&mut self, reader: &mut IvfReader) -> anyhow::Result<ReadFrameResult> {
		loop {
			match self.read_frame_iter(reader)? {
				ReadFrameIterResult::Ok(rgbx_frame) => {
					return Ok(ReadFrameResult::Ok(rgbx_frame));
				}
				ReadFrameIterResult::Again => { /* loop again */ }
				ReadFrameIterResult::EndOfFile => return Ok(ReadFrameResult::EndOfFile),
			}
		}
	}
}
