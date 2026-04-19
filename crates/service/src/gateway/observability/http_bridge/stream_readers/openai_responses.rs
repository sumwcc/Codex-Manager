use super::{
    classify_upstream_stream_read_error, inspect_openai_responses_sse_frame,
    mark_first_response_ms, merge_usage, should_emit_keepalive, stream_idle_timed_out,
    stream_idle_timeout_message, stream_reader_disconnected_message, stream_wait_timeout,
    upstream_hint_or_stream_incomplete_message, Arc, Cursor, Mutex, PassthroughSseCollector,
    Read, SseKeepAliveFrame, SseTerminal,
};
use crate::gateway::upstream::{GatewayByteStream, GatewayByteStreamItem, GatewayStreamResponse};
use eventsource_stream::{Event, Eventsource};
use futures_util::pin_mut;
use futures_util::stream::unfold;
use futures_util::task::noop_waker_ref;
use futures_util::Stream;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::task::{Context, Poll};
use std::thread;
use std::time::Instant;

const OPENAI_RESPONSES_SSE_CHANNEL_CAPACITY: usize = 128;

#[derive(Debug)]
enum OpenAIResponsesSsePumpItem {
    Frame { lines: Vec<String>, bytes: Vec<u8> },
    Eof,
    Error(String),
}

struct OpenAIResponsesSsePump {
    rx: Receiver<OpenAIResponsesSsePumpItem>,
}

impl OpenAIResponsesSsePump {
    fn new(byte_stream: GatewayByteStream) -> Self {
        let (tx, rx) =
            mpsc::sync_channel::<OpenAIResponsesSsePumpItem>(OPENAI_RESPONSES_SSE_CHANNEL_CAPACITY);
        thread::spawn(move || {
            let byte_stream = unfold(Some(byte_stream), |state| async move {
                let byte_stream = state?;
                match byte_stream.recv() {
                    Ok(GatewayByteStreamItem::Chunk(bytes)) => Some((Ok(bytes), Some(byte_stream))),
                    Ok(GatewayByteStreamItem::Eof) => None,
                    Ok(GatewayByteStreamItem::Error(err)) => Some((Err(err), None)),
                    Err(_) => None,
                }
            });

            let stream = byte_stream.eventsource();
            pin_mut!(stream);
            let waker = noop_waker_ref();
            let mut cx = Context::from_waker(waker);

            loop {
                match stream.as_mut().poll_next(&mut cx) {
                    Poll::Ready(Some(Ok(event))) => {
                        let (lines, bytes) = event_to_sse_frame(&event);
                        if tx
                            .send(OpenAIResponsesSsePumpItem::Frame { lines, bytes })
                            .is_err()
                        {
                            return;
                        }
                    }
                    Poll::Ready(Some(Err(err))) => {
                        let _ = tx.send(OpenAIResponsesSsePumpItem::Error(err.to_string()));
                        return;
                    }
                    Poll::Ready(None) => {
                        let _ = tx.send(OpenAIResponsesSsePumpItem::Eof);
                        return;
                    }
                    Poll::Pending => thread::yield_now(),
                }
            }
        });
        Self { rx }
    }

    fn recv_timeout(
        &self,
        timeout: std::time::Duration,
    ) -> Result<OpenAIResponsesSsePumpItem, RecvTimeoutError> {
        self.rx.recv_timeout(timeout)
    }
}

fn event_to_sse_frame(event: &Event) -> (Vec<String>, Vec<u8>) {
    let mut lines = Vec::new();
    if !event.id.is_empty() {
        lines.push(format!("id: {}\n", event.id));
    }
    if let Some(retry) = event.retry {
        lines.push(format!("retry: {}\n", retry.as_millis()));
    }
    if !event.event.is_empty() && !event.event.eq_ignore_ascii_case("message") {
        lines.push(format!("event: {}\n", event.event));
    }
    for data_line in event.data.split('\n') {
        lines.push(format!("data: {data_line}\n"));
    }
    lines.push("\n".to_string());
    let bytes = lines.concat().into_bytes();
    (lines, bytes)
}

pub(crate) struct OpenAIResponsesPassthroughSseReader {
    upstream: OpenAIResponsesSsePump,
    out_cursor: Cursor<Vec<u8>>,
    usage_collector: Arc<Mutex<PassthroughSseCollector>>,
    keepalive_frame: SseKeepAliveFrame,
    request_started_at: Instant,
    last_upstream_activity: Instant,
    saw_upstream_frame: bool,
    finished: bool,
}

impl OpenAIResponsesPassthroughSseReader {
    pub(crate) fn new(
        upstream: reqwest::blocking::Response,
        usage_collector: Arc<Mutex<PassthroughSseCollector>>,
        keepalive_frame: SseKeepAliveFrame,
        request_started_at: Instant,
    ) -> Self {
        Self::from_stream_response(
            GatewayStreamResponse::from_blocking_response(upstream),
            usage_collector,
            keepalive_frame,
            request_started_at,
        )
    }

    pub(crate) fn from_stream_response(
        upstream: GatewayStreamResponse,
        usage_collector: Arc<Mutex<PassthroughSseCollector>>,
        keepalive_frame: SseKeepAliveFrame,
        request_started_at: Instant,
    ) -> Self {
        Self {
            upstream: OpenAIResponsesSsePump::new(upstream.into_body()),
            out_cursor: Cursor::new(Vec::new()),
            usage_collector,
            keepalive_frame,
            request_started_at,
            last_upstream_activity: Instant::now(),
            saw_upstream_frame: false,
            finished: false,
        }
    }

    fn update_usage_from_frame(&self, lines: &[String]) {
        let inspection = inspect_openai_responses_sse_frame(lines);
        if let Ok(mut collector) = self.usage_collector.lock() {
            if let Some(event_type) = inspection.last_event_type {
                collector.last_event_type = Some(event_type);
            }
            if let Some(parsed) = inspection.usage {
                merge_usage(&mut collector.usage, parsed);
            }
            if let Some(terminal) = inspection.terminal {
                collector.saw_terminal = true;
                if let SseTerminal::Err(message) = terminal {
                    collector.terminal_error = Some(message);
                }
            }
        }
    }

    fn next_chunk(&mut self) -> std::io::Result<Vec<u8>> {
        loop {
            match self
                .upstream
                .recv_timeout(stream_wait_timeout(self.last_upstream_activity))
            {
                Ok(OpenAIResponsesSsePumpItem::Frame { lines, bytes }) => {
                    self.last_upstream_activity = Instant::now();
                    self.saw_upstream_frame = true;
                    self.update_usage_from_frame(&lines);
                    mark_first_response_ms(&self.usage_collector, self.request_started_at);
                    return Ok(bytes);
                }
                Ok(OpenAIResponsesSsePumpItem::Eof) => {
                    if let Ok(mut collector) = self.usage_collector.lock() {
                        if !collector.saw_terminal {
                            let hint = collector.upstream_error_hint.clone();
                            collector.terminal_error.get_or_insert_with(|| {
                                upstream_hint_or_stream_incomplete_message(hint.as_deref())
                            });
                        }
                    }
                    self.finished = true;
                    return Ok(Vec::new());
                }
                Ok(OpenAIResponsesSsePumpItem::Error(err)) => {
                    self.last_upstream_activity = Instant::now();
                    if let Ok(mut collector) = self.usage_collector.lock() {
                        collector
                            .terminal_error
                            .get_or_insert_with(|| classify_upstream_stream_read_error(&err));
                    }
                    self.finished = true;
                    return Ok(Vec::new());
                }
                Err(RecvTimeoutError::Timeout) => {
                    if stream_idle_timed_out(self.last_upstream_activity) {
                        if let Ok(mut collector) = self.usage_collector.lock() {
                            collector
                                .terminal_error
                                .get_or_insert_with(stream_idle_timeout_message);
                        }
                        self.finished = true;
                        return Ok(Vec::new());
                    }
                    if should_emit_keepalive(self.saw_upstream_frame) {
                        return Ok(self.keepalive_frame.bytes().to_vec());
                    }
                    continue;
                }
                Err(RecvTimeoutError::Disconnected) => {
                    if let Ok(mut collector) = self.usage_collector.lock() {
                        let hint = collector.upstream_error_hint.clone();
                        collector.terminal_error.get_or_insert_with(|| {
                            hint.unwrap_or_else(stream_reader_disconnected_message)
                        });
                    }
                    self.finished = true;
                    return Ok(Vec::new());
                }
            }
        }
    }
}

impl Read for OpenAIResponsesPassthroughSseReader {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        loop {
            let read = self.out_cursor.read(buf)?;
            if read > 0 {
                return Ok(read);
            }
            if self.finished {
                return Ok(0);
            }
            self.out_cursor = Cursor::new(self.next_chunk()?);
        }
    }
}
