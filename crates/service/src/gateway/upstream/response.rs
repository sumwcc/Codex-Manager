use bytes::Bytes;
use std::io::Read;
use std::sync::mpsc::{self, Receiver};
use std::thread;

const GATEWAY_STREAM_READ_CHUNK_BYTES: usize = 8 * 1024;
const GATEWAY_STREAM_CHANNEL_CAPACITY: usize = 128;

#[derive(Debug)]
pub(crate) enum GatewayByteStreamItem {
    Chunk(Bytes),
    Eof,
    Error(String),
}

#[derive(Debug)]
pub(crate) struct GatewayByteStream {
    rx: Receiver<GatewayByteStreamItem>,
}

impl GatewayByteStream {
    pub(crate) fn from_blocking_response(mut response: reqwest::blocking::Response) -> Self {
        let (tx, rx) = mpsc::sync_channel::<GatewayByteStreamItem>(GATEWAY_STREAM_CHANNEL_CAPACITY);
        thread::spawn(move || {
            loop {
                let mut buffer = vec![0_u8; GATEWAY_STREAM_READ_CHUNK_BYTES];
                match response.read(&mut buffer) {
                    Ok(0) => {
                        let _ = tx.send(GatewayByteStreamItem::Eof);
                        return;
                    }
                    Ok(read) => {
                        buffer.truncate(read);
                        if tx
                            .send(GatewayByteStreamItem::Chunk(Bytes::from(buffer)))
                            .is_err()
                        {
                            return;
                        }
                    }
                    Err(err) => {
                        let _ = tx.send(GatewayByteStreamItem::Error(err.to_string()));
                        return;
                    }
                }
            }
        });
        Self { rx }
    }

    pub(crate) fn recv(&self) -> Result<GatewayByteStreamItem, mpsc::RecvError> {
        self.rx.recv()
    }

    pub(crate) fn read_all_bytes(self) -> Result<Bytes, String> {
        let mut buffer = Vec::new();
        loop {
            match self.rx.recv() {
                Ok(GatewayByteStreamItem::Chunk(bytes)) => buffer.extend_from_slice(bytes.as_ref()),
                Ok(GatewayByteStreamItem::Eof) => return Ok(Bytes::from(buffer)),
                Ok(GatewayByteStreamItem::Error(err)) => return Err(err),
                Err(_) => return Ok(Bytes::from(buffer)),
            }
        }
    }
}

#[derive(Debug)]
pub(crate) struct GatewayStreamResponse {
    status: reqwest::StatusCode,
    headers: reqwest::header::HeaderMap,
    body: GatewayByteStream,
}

impl GatewayStreamResponse {
    pub(crate) fn from_blocking_response(response: reqwest::blocking::Response) -> Self {
        let status = response.status();
        let headers = response.headers().clone();
        let body = GatewayByteStream::from_blocking_response(response);
        Self { status, headers, body }
    }

    pub(crate) fn status(&self) -> reqwest::StatusCode {
        self.status
    }

    pub(crate) fn headers(&self) -> &reqwest::header::HeaderMap {
        &self.headers
    }

    pub(crate) fn read_all_bytes(self) -> Result<Bytes, String> {
        self.body.read_all_bytes()
    }

    pub(crate) fn into_body(self) -> GatewayByteStream {
        self.body
    }
}

#[derive(Debug)]
pub(crate) enum GatewayUpstreamResponse {
    Blocking(reqwest::blocking::Response),
    Stream(GatewayStreamResponse),
}

impl GatewayUpstreamResponse {
    pub(crate) fn status(&self) -> reqwest::StatusCode {
        match self {
            Self::Blocking(response) => response.status(),
            Self::Stream(response) => response.status(),
        }
    }

    pub(crate) fn headers(&self) -> &reqwest::header::HeaderMap {
        match self {
            Self::Blocking(response) => response.headers(),
            Self::Stream(response) => response.headers(),
        }
    }

}

impl From<reqwest::blocking::Response> for GatewayUpstreamResponse {
    fn from(response: reqwest::blocking::Response) -> Self {
        Self::Blocking(response)
    }
}
