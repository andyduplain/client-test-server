mod header_range;
mod tokiort;

use std::{
    env,
    net::SocketAddr,
    path::{Path, PathBuf},
};

#[cfg(unix)]
use std::os::unix::fs::MetadataExt;

#[cfg(windows)]
use std::os::windows::fs::MetadataExt;

use anyhow::{anyhow, Context, Result};
use bytes::Bytes;
use futures_util::TryStreamExt;
use http_body_util::{combinators::BoxBody, BodyExt, Empty, Full, StreamBody};
use hyper::{
    body::{Frame, Incoming},
    header::{HeaderMap, ACCEPT_RANGES, CONTENT_LENGTH, CONTENT_RANGE, CONTENT_TYPE, HOST},
    server::conn::http1,
    service::service_fn,
    Method, Request, Response, StatusCode, Uri,
};
use tokio::{
    fs::{File, OpenOptions},
    io::{AsyncSeekExt, AsyncWriteExt, SeekFrom},
    net::{TcpListener, TcpStream},
};
use tokio_util::io::ReaderStream;

use crate::{header_range::HeaderRange, tokiort::TokioIo};

static mut CONTENT_DIR: Option<PathBuf> = None;

fn content(filename: &str) -> PathBuf {
    unsafe { CONTENT_DIR.as_ref().unwrap().join(filename) }
}

#[tokio::main]
async fn main() -> Result<()> {
    if env::var("RUST_LOG").is_err() {
        env::set_var("RUST_LOG", "error,client_test_server=debug");
    }
    env_logger::init();

    let content_dir = find_content_dir()?;
    log::debug!("Will serve content from {:?}", content_dir);

    unsafe {
        CONTENT_DIR = Some(content_dir);
    }

    // Download the video file; we don't want to keep such a big file in git
    download(&Uri::from_static("http://distribution.bbb3d.renderfarming.net/video/mp4/bbb_sunflower_2160p_60fps_normal.mp4"), &content("bbb.mp4")).await?;

    let addr: SocketAddr = "127.0.0.1:1337".parse().unwrap();

    let listener = TcpListener::bind(addr).await?;
    log::info!("Listening on http://{}", addr);

    loop {
        let (stream, _) = listener.accept().await?;
        let io = TokioIo::new(stream);

        tokio::task::spawn(async move {
            if let Err(err) = http1::Builder::new()
                .serve_connection(io, service_fn(server))
                .await
            {
                log::error!("Failed to serve connection: {:?}", err);
            }
        });
    }
}

async fn download(url: &Uri, filename: &Path) -> Result<()> {
    if filename.is_file() {
        log::info!("File {:?} is already downloaded", filename);
        return Ok(());
    } else {
        log::info!("Downloading {:?} from {:?}", filename, url)
    }

    let mut file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(filename)
        .await
        .context(anyhow!("Failed to open file {:?} for writing", filename))?;

    let host = url.host().context("Download URL has no host")?;
    let port = url.port_u16().unwrap_or(80);
    let addr = format!("{}:{}", host, port);
    let stream = TcpStream::connect(addr).await?;
    let io = TokioIo::new(stream);

    let (mut sender, conn) = hyper::client::conn::http1::handshake(io).await?;
    tokio::task::spawn(async move {
        if let Err(err) = conn.await {
            log::error!("Failed to download; connection failed: {:?}", err);
        }
    });

    let authority = url
        .authority()
        .context("Failed to get URL authority")?
        .clone();

    let path = url.path();
    let req = Request::builder()
        .uri(path)
        .header(HOST, authority.as_str())
        .body(Empty::<Bytes>::new())?;

    let mut res = sender.send_request(req).await?;

    let content_length = if let Some(content_length_header) = res.headers().get(CONTENT_LENGTH) {
        content_length_header.to_str()?.parse::<usize>()?
    } else {
        0
    };
    let mut total_downloaded = 0;
    let mut printed_pct = 0;

    // Stream the body, writing each chunk to stdout as we get it
    // (instead of buffering and printing at the end).
    while let Some(next) = res.frame().await {
        let frame = next?;
        if let Some(chunk) = frame.data_ref() {
            file.write_all(chunk).await?;
            total_downloaded += chunk.len();
            let pct = ((total_downloaded as f32 / content_length as f32) * 100.0) as u32;
            if pct > printed_pct {
                log::info!("{pct}% downloaded");
                printed_pct = pct;
            }
        }
    }

    if total_downloaded == content_length {
        log::info!("Download of {:?} complete", filename);
        Ok(())
    } else {
        log::error!("Failed to download {:?}", filename);
        Err(anyhow!("Failed to download file"))
    }
}

async fn server(req: Request<Incoming>) -> Result<Response<BoxBody<Bytes, std::io::Error>>> {
    let method = req.method();
    let path = req.uri().path();
    let headers = req.headers();
    log::debug!("Request: {method} {path}");

    match (method, path) {
        (&Method::GET, "/") => {
            let index_html = content("index.html");
            send_file(headers, &index_html, "text/html; charset=utf-8").await
        }
        (&Method::GET, "/video") => {
            let bbb_mp4 = content("bbb.mp4");
            send_file(headers, &bbb_mp4, "video/mp4").await
        }
        _ => Ok(not_found()),
    }
}

async fn send_file(
    req_headers: &HeaderMap,
    filename: &Path,
    content_type: &str,
) -> Result<Response<BoxBody<Bytes, std::io::Error>>> {
    let mut resp_headers = HeaderMap::new();
    resp_headers.append(CONTENT_TYPE, content_type.try_into()?);

    let mut file = File::open(filename)
        .await
        .context(anyhow!("Failed to open file {:?} for reading", filename))?;

    let (status, reader_stream) = if let Some(range) = req_headers.get("Range") {
        // Send partial content
        let range = HeaderRange::from_header_value(range)?;
        if range.units != "bytes" {
            return Err(anyhow!(
                "Invalid request range units; only 'bytes' are supported"
            ));
        }

        const CHUNK_SIZE: u64 = 1_000_000;
        let file_size = get_file_size(filename);
        let start = range.start;
        let end = (start + CHUNK_SIZE - 1).min(file_size - 1);
        let content_length = end - start + 1;
        resp_headers.append(
            CONTENT_RANGE,
            format!("bytes {start}-{end}/{file_size}").try_into()?,
        );
        resp_headers.append(ACCEPT_RANGES, "bytes".try_into()?);
        resp_headers.append(CONTENT_LENGTH, content_length.into());

        log::debug!(
            "req_headers={:?} resp_headers={:?}",
            req_headers,
            resp_headers
        );

        file.seek(SeekFrom::Start(start))
            .await
            .context(anyhow!("Failed to seek file to {}", range.start))?;

        (StatusCode::PARTIAL_CONTENT, ReaderStream::new(file))
    } else {
        // Send all the content
        (StatusCode::OK, ReaderStream::new(file))
    };

    let stream_body = StreamBody::new(reader_stream.map_ok(Frame::data));
    let boxed_body = stream_body.boxed();
    let mut response_builder = Response::builder().status(status);
    *response_builder.headers_mut().unwrap() = resp_headers;

    let response = response_builder
        .body(boxed_body)
        .context("Failed to build response")?;

    Ok(response)
}

/// HTTP status code 404
fn not_found() -> Response<BoxBody<Bytes, std::io::Error>> {
    Response::builder()
        .status(StatusCode::NOT_FOUND)
        .body(
            Full::new("Not Found".into())
                .map_err(|e| match e {})
                .boxed(),
        )
        .unwrap()
}

// Try and find the content directory in the same directory as the executable
// and in several ancestor directories.
fn find_content_dir() -> Result<PathBuf> {
    let exe = env::current_exe().context("Failed to get executable's file name")?;
    let mut parent_dir = exe.parent();
    for _ in 0..3 {
        if let Some(some_parent_dir) = parent_dir {
            let candidate = some_parent_dir.join("content");
            if candidate.is_dir() {
                return Ok(candidate);
            }
            parent_dir = some_parent_dir.parent();
        } else {
            break;
        }
    }
    Err(anyhow!("Failed to find content directory"))
}

#[cfg(unix)]
fn get_file_size(filename: &Path) -> u64 {
    if let Ok(md) = filename.metadata() {
        md.size()
    } else {
        0
    }
}

#[cfg(windows)]
fn get_file_size(filename: &Path) -> u64 {
    if let Ok(md) = filename.metadata() {
        md.file_size()
    } else {
        0
    }
}
