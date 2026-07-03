//
// example smol+hyper usage derived from
// https://github.com/smol-rs/smol/blob/master/examples/hyper-client.rs
// under Apache-2.0 + MIT license.
// Repository URL: https://github.com/smol-rs/smol
//

use anyhow::Context as _;
use async_native_tls::TlsStream;
use http_body_util::{BodyStream, Empty};
use hyper::Request;
use smol::{net::TcpStream, prelude::*};
use std::convert::TryInto;
use std::pin::Pin;
use std::task::{Context, Poll};
use wlx_common::async_executor::AsyncExecutor;
pub struct HttpClientResponse {
	pub data: Vec<u8>,
}

impl HttpClientResponse {
	pub fn into_json<T>(self) -> anyhow::Result<T>
	where
		T: for<'a> serde::Deserialize<'a>,
	{
		let utf8 = str::from_utf8(&self.data)?;
		Ok(serde_json::from_str::<T>(utf8)?)
	}
}

pub struct ProgressFuncData {
	pub bytes_downloaded: u64,
	pub file_size: u64,
}

pub type ProgressFunc = Box<dyn Fn(ProgressFuncData)>;

pub struct GetParams<'a> {
	pub executor: &'a AsyncExecutor,
	pub url: &'a str,
	pub on_progress: Option<ProgressFunc>,
}

pub async fn get(params: GetParams<'_>) -> anyhow::Result<HttpClientResponse> {
	let url: hyper::Uri = params.url.try_into()?;
	let req = Request::builder()
		.header(
			hyper::header::HOST,
			url.authority().context("invalid authority")?.clone().as_str(),
		)
		.uri(url)
		.body(Empty::new())?;

	let resp = fetch_and_follow_redirects(params.executor, req, params.on_progress, &params.url).await?;

	Ok(resp)
}

async fn fetch_and_follow_redirects(
	executor: &AsyncExecutor,
	req: hyper::Request<Empty<&'static [u8]>>,
	mut on_progress: Option<ProgressFunc>,
	initial_url: &str,
) -> anyhow::Result<HttpClientResponse> {
	log::info!("fetching URL \"{}\"", initial_url);

	let resp = fetch(executor, req.clone()).await?;
	let status = resp.status();

	if status.is_redirection() {
		let next_req = follow_single_redirect(&req, &resp).await?;

		let max_redirects = 10;
		let mut current_req = next_req;
		let mut redirects = 0;
		loop {
			let resp = fetch(executor, current_req.clone()).await?;
			let resp_status = resp.status();

			if resp_status.is_success() {
				let (parts, body) = resp.into_parts();

				let mut bytes_downloaded: u64 = 0;
				let mut file_size: u64 = 1;

				if let Some(val) = parts.headers.get("Content-Length")
					&& let Ok(str) = val.to_str()
					&& let Ok(s) = str.parse()
				{
					file_size = s;
				}

				let data = BodyStream::new(body)
					.try_fold(Vec::new(), |mut body, chunk| {
						if let Some(chunk) = chunk.data_ref() {
							bytes_downloaded += chunk.len() as u64;
							body.extend_from_slice(chunk);

							if let Some(on_progress) = &mut on_progress {
								on_progress(ProgressFuncData {
									bytes_downloaded,
									file_size,
								})
							}
						}
						Ok(body)
					})
					.await?;

				return Ok(HttpClientResponse { data });
			} else if resp_status.is_redirection() {
				redirects += 1;
				if redirects >= max_redirects {
					anyhow::bail!("too many redirects");
				}
				current_req = follow_single_redirect(&current_req, &resp).await?;
			} else {
				anyhow::bail!("non-200 HTTP response: {}", resp_status.as_str());
			}
		}
	}

	if !resp.status().is_success() {
		// non-200 HTTP response
		anyhow::bail!("non-200 HTTP response: {}", resp.status().as_str());
	}

	let mut bytes_downloaded: u64 = 0;
	let mut file_size: u64 = 1;

	let (parts, body) = resp.into_parts();

	// that's a pretty interesting way to get file size :]
	if let Some(val) = parts.headers.get("Content-Length")
		&& let Ok(str) = val.to_str()
		&& let Ok(s) = str.parse()
	{
		file_size = s;
	}

	let data = BodyStream::new(body)
		.try_fold(Vec::new(), |mut body, chunk| {
			if let Some(chunk) = chunk.data_ref() {
				bytes_downloaded += chunk.len() as u64;
				body.extend_from_slice(chunk);

				if let Some(on_progress) = &mut on_progress {
					on_progress(ProgressFuncData {
						bytes_downloaded,
						file_size,
					})
				}
			}
			Ok(body)
		})
		.await?;

	Ok(HttpClientResponse { data })
}

fn uri_try_from_str(s: &str) -> anyhow::Result<hyper::http::uri::PathAndQuery> {
	use std::convert::TryInto;
	let uri: hyper::Uri = s.try_into().context("invalid URI")?;
	uri
		.path_and_query()
		.ok_or_else(|| anyhow::anyhow!("URI has no path and query component"))
		.cloned()
}

async fn follow_single_redirect(
	req: &hyper::Request<Empty<&'static [u8]>>,
	resp: &hyper::Response<hyper::body::Incoming>,
) -> anyhow::Result<hyper::Request<Empty<&'static [u8]>>> {
	let location = resp
		.headers()
		.get(hyper::header::LOCATION)
		.ok_or_else(|| anyhow::anyhow!("redirect response has no Location header"))?;

	let location_str = location.to_str().context("invalid redirect location header")?;

	// resolve relative urls against the original url.
	let original_uri = req.uri().clone();
	let next_url: hyper::Uri = if location_str.starts_with("http://") || location_str.starts_with("https://") {
		hyper::Uri::try_from(location_str).context("invalid redirect location")?
	} else {
		let mut parts = original_uri.into_parts();
		parts.path_and_query = uri_try_from_str(location_str)
			.context("invalid redirect location")?
			.into();
		hyper::Uri::from_parts(parts).context("failed to construct redirect URI")?
	};

	log::info!("redirecting to \"{}\"", next_url);

	let next_req = Request::builder()
		.header(
			hyper::header::HOST,
			next_url.authority().context("invalid authority")?.as_str(),
		)
		.uri(next_url)
		.body(Empty::new())?;

	Ok(next_req)
}

pub async fn get_simple(executor: &AsyncExecutor, url: &str) -> anyhow::Result<HttpClientResponse> {
	get(GetParams {
		executor,
		url,
		on_progress: None,
	})
	.await
}

async fn fetch(
	ex: &AsyncExecutor,
	req: hyper::Request<http_body_util::Empty<&'static [u8]>>,
) -> anyhow::Result<hyper::Response<hyper::body::Incoming>> {
	let io = {
		let host = req.uri().host().context("cannot parse host")?;

		match req.uri().scheme_str() {
			Some("http") => {
				let stream = {
					let port = req.uri().port_u16().unwrap_or(80);
					smol::net::TcpStream::connect((host, port)).await?
				};
				SmolStream::Plain(stream)
			}
			Some("https") => {
				// In case of HTTPS, establish a secure TLS connection first.
				let stream = {
					let port = req.uri().port_u16().unwrap_or(443);
					smol::net::TcpStream::connect((host, port)).await?
				};
				let stream = async_native_tls::connect(host, stream).await?;
				SmolStream::Tls(stream)
			}
			scheme => anyhow::bail!("unsupported scheme: {:?}", scheme),
		}
	};

	// Spawn the HTTP/1 connection.
	let (mut sender, conn) = hyper::client::conn::http1::handshake(smol_hyper::rt::FuturesIo::new(io)).await?;
	ex.spawn(async move {
		if let Err(e) = conn.await {
			println!("Connection failed: {:?}", e);
		}
	})
	.detach();

	// Get the result
	let result = sender.send_request(req).await?;
	Ok(result)
}

/// A TCP or TCP+TLS connection.
enum SmolStream {
	/// A plain TCP connection.
	Plain(TcpStream),

	/// A TCP connection secured by TLS.
	Tls(TlsStream<TcpStream>),
}

impl AsyncRead for SmolStream {
	fn poll_read(mut self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &mut [u8]) -> Poll<smol::io::Result<usize>> {
		match &mut *self {
			SmolStream::Plain(stream) => Pin::new(stream).poll_read(cx, buf),
			SmolStream::Tls(stream) => Pin::new(stream).poll_read(cx, buf),
		}
	}
}

impl AsyncWrite for SmolStream {
	fn poll_write(mut self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &[u8]) -> Poll<smol::io::Result<usize>> {
		match &mut *self {
			SmolStream::Plain(stream) => Pin::new(stream).poll_write(cx, buf),
			SmolStream::Tls(stream) => Pin::new(stream).poll_write(cx, buf),
		}
	}

	fn poll_close(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<smol::io::Result<()>> {
		match &mut *self {
			SmolStream::Plain(stream) => Pin::new(stream).poll_close(cx),
			SmolStream::Tls(stream) => Pin::new(stream).poll_close(cx),
		}
	}

	fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<smol::io::Result<()>> {
		match &mut *self {
			SmolStream::Plain(stream) => Pin::new(stream).poll_flush(cx),
			SmolStream::Tls(stream) => Pin::new(stream).poll_flush(cx),
		}
	}
}
