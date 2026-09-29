use std::collections::HashSet;
use std::io;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, mpsc};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use inputflow_windows::platform::pipe::{NamedPipeListener, PipeShutdown};
use serde_json::Value;

use crate::{
    CodecError, ErrorBody, EventSubscription, HandlerReply, HandshakeParams, HandshakeResult,
    MAX_CONNECTIONS, MAX_REQUEST_ID_BYTES, MAX_REQUESTS_PER_CONNECTION, PROTOCOL_VERSION,
    RequestEnvelope, ResponseEnvelope, SubscriptionResult, parse_params, read_json_frame, to_value,
    write_json_frame,
};

const EVENT_HEARTBEAT_INTERVAL: Duration = Duration::from_secs(1);
const SHUTDOWN_NOTICE_INTERVAL: Duration = Duration::from_millis(50);

pub trait ProtocolHandler: Send + Sync + 'static {
    fn handshake(&self, params: HandshakeParams) -> Result<HandshakeResult, ErrorBody>;

    fn handle(
        &self,
        connection_id: u64,
        method: &str,
        params: Value,
    ) -> Result<HandlerReply, ErrorBody>;

    fn disconnected(&self, _connection_id: u64) {}

    fn server_shutting_down(&self) {}
}

pub struct ProtocolServer;

pub struct ProtocolServerHandle {
    closing: Arc<AtomicBool>,
    shutdown: PipeShutdown,
    handler: Arc<dyn ProtocolHandler>,
    thread: Option<JoinHandle<Result<(), String>>>,
}

impl ProtocolServer {
    pub fn start(
        pipe_name: impl Into<String>,
        handler: Arc<dyn ProtocolHandler>,
    ) -> Result<ProtocolServerHandle, String> {
        let shutdown = PipeShutdown::new()?;
        let listener =
            NamedPipeListener::bind(pipe_name.into(), MAX_CONNECTIONS, shutdown.clone())?;
        let closing = Arc::new(AtomicBool::new(false));
        let thread_closing = Arc::clone(&closing);
        let thread_handler = Arc::clone(&handler);
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        let thread = thread::spawn(move || {
            run_accept_loop(listener, thread_handler, thread_closing, ready_tx)
        });
        match ready_rx.recv() {
            Ok(Ok(())) => Ok(ProtocolServerHandle {
                closing,
                shutdown,
                handler,
                thread: Some(thread),
            }),
            Ok(Err(error)) => {
                let _ = thread.join();
                Err(error)
            }
            Err(_) => {
                let detail = thread
                    .join()
                    .err()
                    .map(|_| " (server thread panicked)")
                    .unwrap_or_default();
                Err(format!(
                    "Named Pipe server ended before reporting readiness{detail}"
                ))
            }
        }
    }
}

impl ProtocolServerHandle {
    pub fn shutdown(&mut self) -> Result<(), String> {
        let Some(thread) = self.thread.take() else {
            return Ok(());
        };
        self.closing.store(true, Ordering::Release);
        self.handler.server_shutting_down();
        thread::sleep(SHUTDOWN_NOTICE_INTERVAL);
        self.shutdown.signal()?;
        match thread.join() {
            Ok(result) => result,
            Err(_) => Err("Named Pipe server thread panicked during shutdown".to_string()),
        }
    }
}

impl Drop for ProtocolServerHandle {
    fn drop(&mut self) {
        let _ = self.shutdown();
    }
}

fn run_accept_loop(
    mut listener: NamedPipeListener,
    handler: Arc<dyn ProtocolHandler>,
    closing: Arc<AtomicBool>,
    ready: mpsc::SyncSender<Result<(), String>>,
) -> Result<(), String> {
    let mut workers: Vec<JoinHandle<()>> = Vec::new();
    let next_connection_id = AtomicU64::new(1);
    let mut ready = Some(ready);

    while !closing.load(Ordering::Acquire) {
        workers.retain(|worker| !worker.is_finished());
        if workers.len() >= MAX_CONNECTIONS as usize {
            thread::sleep(Duration::from_millis(10));
            continue;
        }
        let stream_result = if ready.is_some() {
            listener.accept_after_create(|| {
                if let Some(ready) = ready.take() {
                    let _ = ready.send(Ok(()));
                }
            })
        } else {
            listener.accept()
        };
        let stream = match stream_result {
            Ok(stream) => stream,
            Err(error)
                if closing.load(Ordering::Acquire)
                    || matches!(
                        error.kind(),
                        io::ErrorKind::Interrupted | io::ErrorKind::ConnectionAborted
                    ) =>
            {
                break;
            }
            Err(error) => {
                let message = format!("Named Pipe accept failed: {error}");
                if let Some(ready) = ready.take() {
                    let _ = ready.send(Err(message.clone()));
                }
                return Err(message);
            }
        };
        if closing.load(Ordering::Acquire) {
            break;
        }
        let connection_id = next_connection_id.fetch_add(1, Ordering::Relaxed);
        let connection_handler = Arc::clone(&handler);
        let connection_closing = Arc::clone(&closing);
        workers.push(thread::spawn(move || {
            serve_connection(
                stream,
                connection_id,
                connection_handler,
                connection_closing,
            );
        }));
    }

    for worker in workers {
        let _ = worker.join();
    }
    Ok(())
}

fn serve_connection<S>(
    mut stream: S,
    connection_id: u64,
    handler: Arc<dyn ProtocolHandler>,
    closing: Arc<AtomicBool>,
) where
    S: io::Read + io::Write,
{
    struct DisconnectGuard {
        connection_id: u64,
        handler: Arc<dyn ProtocolHandler>,
    }
    impl Drop for DisconnectGuard {
        fn drop(&mut self) {
            self.handler.disconnected(self.connection_id);
        }
    }

    let _guard = DisconnectGuard {
        connection_id,
        handler: Arc::clone(&handler),
    };
    let mut handshaken = false;
    let mut request_ids = HashSet::new();
    loop {
        let request = match read_json_frame::<_, RequestEnvelope>(&mut stream) {
            Ok(Some(request)) => request,
            Ok(None) => break,
            Err(error) => {
                if matches!(
                    &error,
                    CodecError::Io(io_error)
                        if matches!(
                            io_error.kind(),
                            io::ErrorKind::Interrupted
                                | io::ErrorKind::BrokenPipe
                                | io::ErrorKind::ConnectionReset
                                | io::ErrorKind::UnexpectedEof
                        )
                ) {
                    break;
                }
                let response = ResponseEnvelope::error("", codec_error_body(&error));
                let _ = write_json_frame(&mut stream, &response);
                if error.connection_fatal() {
                    break;
                }
                continue;
            }
        };

        let request_id = request.request_id.clone();
        if request.protocol_version != PROTOCOL_VERSION {
            let response = ResponseEnvelope::error(
                request_id,
                ErrorBody::new(
                    "unsupported_protocol",
                    format!(
                        "protocol version {} is unsupported; expected {PROTOCOL_VERSION}",
                        request.protocol_version
                    ),
                ),
            );
            let _ = write_json_frame(&mut stream, &response);
            break;
        }
        if request_id.is_empty() || request_id.len() > MAX_REQUEST_ID_BYTES {
            let response = ResponseEnvelope::error(
                request_id,
                ErrorBody::new(
                    "invalid_request_id",
                    format!("request_id must contain 1 to {MAX_REQUEST_ID_BYTES} UTF-8 bytes"),
                ),
            );
            if write_json_frame(&mut stream, &response).is_err() {
                break;
            }
            continue;
        }
        if request_ids.len() >= MAX_REQUESTS_PER_CONNECTION {
            let response = ResponseEnvelope::error(
                request_id,
                ErrorBody::new(
                    "connection_request_limit",
                    "this connection reached its request limit; reconnect before continuing",
                ),
            );
            let _ = write_json_frame(&mut stream, &response);
            break;
        }
        if !request_ids.insert(request_id.clone()) {
            let response = ResponseEnvelope::error(
                request_id,
                ErrorBody::new(
                    "duplicate_request_id",
                    "request_id values must be unique within one connection",
                ),
            );
            if write_json_frame(&mut stream, &response).is_err() {
                break;
            }
            continue;
        }
        if closing.load(Ordering::Acquire) {
            let response = ResponseEnvelope::error(
                request_id,
                ErrorBody::new("agent_shutting_down", "the agent is shutting down"),
            );
            let _ = write_json_frame(&mut stream, &response);
            break;
        }

        if !handshaken && request.method != "handshake" {
            let response = ResponseEnvelope::error(
                request_id,
                ErrorBody::new(
                    "handshake_required",
                    "handshake must be the first request on a connection",
                ),
            );
            if write_json_frame(&mut stream, &response).is_err() {
                break;
            }
            continue;
        }

        if request.method == "handshake" {
            let response = if handshaken {
                ResponseEnvelope::error(
                    request_id,
                    ErrorBody::new(
                        "already_handshaken",
                        "handshake has already completed on this connection",
                    ),
                )
            } else {
                match parse_params::<HandshakeParams>(request.params)
                    .and_then(|params| handler.handshake(params))
                    .and_then(to_value)
                {
                    Ok(result) => {
                        handshaken = true;
                        ResponseEnvelope::success(request_id, result)
                    }
                    Err(error) => ResponseEnvelope::error(request_id, error),
                }
            };
            if write_json_frame(&mut stream, &response).is_err() {
                break;
            }
            continue;
        }

        let result = handler.handle(connection_id, &request.method, request.params);
        match result {
            Ok(HandlerReply::Response(result)) => {
                let response = ResponseEnvelope::success(request_id, result);
                if write_json_frame(&mut stream, &response).is_err() {
                    break;
                }
            }
            Ok(HandlerReply::Subscription(subscription)) => {
                let result = match to_value(SubscriptionResult {
                    subscription_id: subscription.id(),
                    queue_capacity: crate::EVENT_QUEUE_CAPACITY,
                }) {
                    Ok(result) => result,
                    Err(error) => {
                        let _ = write_json_frame(
                            &mut stream,
                            &ResponseEnvelope::error(request_id, error),
                        );
                        break;
                    }
                };
                if write_json_frame(&mut stream, &ResponseEnvelope::success(request_id, result))
                    .is_err()
                {
                    break;
                }
                stream_events(&mut stream, subscription, &closing);
                break;
            }
            Err(error) => {
                let response = ResponseEnvelope::error(request_id, error);
                if write_json_frame(&mut stream, &response).is_err() {
                    break;
                }
            }
        }
    }
}

fn stream_events<S>(stream: &mut S, subscription: EventSubscription, closing: &AtomicBool)
where
    S: io::Write,
{
    loop {
        let event = if closing.load(Ordering::Acquire) {
            match subscription.receiver().try_recv() {
                Ok(event) => event,
                Err(mpsc::TryRecvError::Empty | mpsc::TryRecvError::Disconnected) => break,
            }
        } else {
            match subscription
                .receiver()
                .recv_timeout(EVENT_HEARTBEAT_INTERVAL)
            {
                Ok(event) => event,
                Err(mpsc::RecvTimeoutError::Timeout) => match subscription.heartbeat_or_pending() {
                    Ok(event) => event,
                    Err(mpsc::TryRecvError::Empty) => continue,
                    Err(mpsc::TryRecvError::Disconnected) => break,
                },
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }
        };
        let shutting_down = matches!(event.event, crate::EventPayload::ServerShuttingDown);
        if write_json_frame(stream, &event).is_err() {
            break;
        }
        if shutting_down {
            break;
        }
    }
}

fn codec_error_body(error: &CodecError) -> ErrorBody {
    match error {
        CodecError::FrameTooLarge { .. } => {
            ErrorBody::new("message_too_large", "frame exceeds the protocol size limit")
                .with_details(error.to_string())
        }
        CodecError::EmptyFrame
        | CodecError::TruncatedHeader
        | CodecError::TruncatedPayload { .. } => {
            ErrorBody::new("malformed_frame", "frame boundary is invalid")
                .with_details(error.to_string())
        }
        CodecError::InvalidJson(_) => {
            ErrorBody::new("invalid_request", "frame is not a valid request envelope")
                .with_details(error.to_string())
        }
        CodecError::Io(_) => ErrorBody::new("transport_error", "pipe transport failed")
            .with_details(error.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use std::fs::{File, OpenOptions};
    use std::io::Write;
    use std::sync::atomic::AtomicUsize;
    use std::time::{Instant, SystemTime, UNIX_EPOCH};

    use super::*;
    use crate::{
        CLIENT_RESPONSE_TIMEOUT_MS, EVENT_QUEUE_CAPACITY, EmptyParams, EventHub, EventPayload,
        HandshakeResult, ResponseBody,
    };

    struct MockHandler {
        disconnected: AtomicUsize,
        events: EventHub,
    }

    impl MockHandler {
        fn new() -> Self {
            Self {
                disconnected: AtomicUsize::new(0),
                events: EventHub::new(),
            }
        }
    }

    impl ProtocolHandler for MockHandler {
        fn handshake(&self, params: HandshakeParams) -> Result<HandshakeResult, ErrorBody> {
            if params.client_name.is_empty() {
                return Err(ErrorBody::new(
                    "invalid_client",
                    "client_name must not be empty",
                ));
            }
            Ok(HandshakeResult {
                server_name: "test".to_string(),
                server_version: "0".to_string(),
                protocol_version: PROTOCOL_VERSION,
                schema_version: 2,
                capabilities: vec!["test".to_string()],
                max_frame_bytes: crate::MAX_FRAME_BYTES,
                max_requests_per_connection: MAX_REQUESTS_PER_CONNECTION,
                event_queue_capacity: EVENT_QUEUE_CAPACITY,
                client_response_timeout_ms: CLIENT_RESPONSE_TIMEOUT_MS,
            })
        }

        fn handle(
            &self,
            _connection_id: u64,
            method: &str,
            params: Value,
        ) -> Result<HandlerReply, ErrorBody> {
            match method {
                "get_status" => {
                    parse_params::<EmptyParams>(params)?;
                    Ok(HandlerReply::Response(serde_json::json!({"ready": true})))
                }
                "slow" => {
                    thread::sleep(Duration::from_millis(150));
                    Ok(HandlerReply::Response(serde_json::json!({"done": true})))
                }
                "subscribe_events" => {
                    parse_params::<EmptyParams>(params)?;
                    self.events.subscribe().map(HandlerReply::Subscription)
                }
                _ => Err(ErrorBody::new("unknown_method", "unknown test method")),
            }
        }

        fn disconnected(&self, _connection_id: u64) {
            self.disconnected.fetch_add(1, Ordering::Relaxed);
        }

        fn server_shutting_down(&self) {
            self.events.publish(EventPayload::ServerShuttingDown);
        }
    }

    fn unique_pipe_name(label: &str) -> String {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        format!(
            "InputFlow.Protocol.Test.{}.{}.{}",
            std::process::id(),
            label,
            nonce
        )
    }

    fn connect(pipe_name: &str) -> File {
        let path = format!(r"\\.\pipe\{pipe_name}");
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            match OpenOptions::new().read(true).write(true).open(&path) {
                Ok(file) => return file,
                Err(error) if Instant::now() < deadline => {
                    let _ = error;
                    thread::sleep(Duration::from_millis(10));
                }
                Err(error) => panic!("failed to connect to {path}: {error}"),
            }
        }
    }

    fn request(id: &str, method: &str, params: Value) -> RequestEnvelope {
        RequestEnvelope {
            protocol_version: PROTOCOL_VERSION,
            request_id: id.to_string(),
            method: method.to_string(),
            params,
        }
    }

    fn handshake(stream: &mut File, id: &str) -> ResponseEnvelope {
        write_json_frame(
            stream,
            &request(
                id,
                "handshake",
                serde_json::json!({"client_name":"rust-test","client_version":"1"}),
            ),
        )
        .unwrap();
        read_json_frame(stream).unwrap().unwrap()
    }

    fn assert_success(response: &ResponseEnvelope) {
        assert!(matches!(response.body, ResponseBody::Success { .. }));
    }

    fn assert_pipe_closed(stream: &mut File) {
        match read_json_frame::<_, Value>(stream) {
            Ok(None) => {}
            Err(CodecError::Io(error))
                if matches!(
                    error.raw_os_error(),
                    Some(code) if code == windows_sys::Win32::Foundation::ERROR_PIPE_NOT_CONNECTED as i32
                        || code == windows_sys::Win32::Foundation::ERROR_BROKEN_PIPE as i32
                ) => {}
            other => panic!("expected a closed pipe, got {other:?}"),
        }
    }

    #[test]
    fn actual_pipe_handles_partial_frames_bad_json_and_reconnect() {
        let name = unique_pipe_name("partial");
        let handler = Arc::new(MockHandler::new());
        let mut server = ProtocolServer::start(name.clone(), handler.clone()).unwrap();
        let mut stream = connect(&name);

        let payload = serde_json::to_vec(&request(
            "h1",
            "handshake",
            serde_json::json!({"client_name":"partial","client_version":"1"}),
        ))
        .unwrap();
        let header = (payload.len() as u32).to_le_bytes();
        stream.write_all(&header[..2]).unwrap();
        stream.write_all(&header[2..]).unwrap();
        for chunk in payload.chunks(3) {
            stream.write_all(chunk).unwrap();
        }
        assert_success(&read_json_frame(&mut stream).unwrap().unwrap());

        stream.write_all(&5_u32.to_le_bytes()).unwrap();
        stream.write_all(b"{oops").unwrap();
        let invalid: ResponseEnvelope = read_json_frame(&mut stream).unwrap().unwrap();
        assert!(matches!(
            invalid.body,
            ResponseBody::Error { ref error } if error.code == "invalid_request"
        ));
        write_json_frame(
            &mut stream,
            &request("s1", "get_status", serde_json::json!({})),
        )
        .unwrap();
        assert_success(&read_json_frame(&mut stream).unwrap().unwrap());
        drop(stream);

        let mut reconnected = connect(&name);
        assert_success(&handshake(&mut reconnected, "h2"));
        drop(reconnected);
        server.shutdown().unwrap();
        assert!(handler.disconnected.load(Ordering::Relaxed) >= 2);
    }

    #[test]
    fn oversized_and_version_mismatch_are_rejected_and_closed() {
        let name = unique_pipe_name("reject");
        let handler = Arc::new(MockHandler::new());
        let mut server = ProtocolServer::start(name.clone(), handler).unwrap();

        let mut oversized = connect(&name);
        oversized
            .write_all(&((crate::MAX_FRAME_BYTES + 1) as u32).to_le_bytes())
            .unwrap();
        let response: ResponseEnvelope = read_json_frame(&mut oversized).unwrap().unwrap();
        assert!(matches!(
            response.body,
            ResponseBody::Error { ref error } if error.code == "message_too_large"
        ));
        assert_pipe_closed(&mut oversized);

        let mut wrong_version = connect(&name);
        let mut bad = request(
            "bad-version",
            "handshake",
            serde_json::json!({"client_name":"old","client_version":"0"}),
        );
        bad.protocol_version = PROTOCOL_VERSION + 1;
        write_json_frame(&mut wrong_version, &bad).unwrap();
        let response: ResponseEnvelope = read_json_frame(&mut wrong_version).unwrap().unwrap();
        assert!(matches!(
            response.body,
            ResponseBody::Error { ref error } if error.code == "unsupported_protocol"
        ));
        assert_pipe_closed(&mut wrong_version);
        server.shutdown().unwrap();
    }

    #[test]
    fn duplicate_ids_do_not_dispatch_twice() {
        let name = unique_pipe_name("duplicate");
        let handler = Arc::new(MockHandler::new());
        let mut server = ProtocolServer::start(name.clone(), handler).unwrap();
        let mut stream = connect(&name);
        assert_success(&handshake(&mut stream, "same"));
        write_json_frame(
            &mut stream,
            &request("same", "get_status", serde_json::json!({})),
        )
        .unwrap();
        let response: ResponseEnvelope = read_json_frame(&mut stream).unwrap().unwrap();
        assert!(matches!(
            response.body,
            ResponseBody::Error { ref error } if error.code == "duplicate_request_id"
        ));
        server.shutdown().unwrap();
    }

    #[test]
    fn concurrent_clients_disconnect_and_server_shutdown_are_bounded() {
        let name = unique_pipe_name("concurrent");
        let handler = Arc::new(MockHandler::new());
        let mut server = ProtocolServer::start(name.clone(), handler.clone()).unwrap();
        let mut clients = Vec::new();
        for index in 0..4 {
            let pipe = name.clone();
            clients.push(thread::spawn(move || {
                let mut stream = connect(&pipe);
                assert_success(&handshake(&mut stream, &format!("h{index}")));
                write_json_frame(
                    &mut stream,
                    &request(&format!("s{index}"), "get_status", serde_json::json!({})),
                )
                .unwrap();
                assert_success(&read_json_frame(&mut stream).unwrap().unwrap());
            }));
        }
        for client in clients {
            client.join().unwrap();
        }
        server.shutdown().unwrap();
        assert!(handler.disconnected.load(Ordering::Relaxed) >= 4);
    }

    #[test]
    fn client_timeout_abandonment_does_not_block_reconnect() {
        let name = unique_pipe_name("timeout");
        let handler = Arc::new(MockHandler::new());
        let mut server = ProtocolServer::start(name.clone(), handler).unwrap();
        let mut stream = connect(&name);
        assert_success(&handshake(&mut stream, "h1"));
        write_json_frame(&mut stream, &request("slow", "slow", serde_json::json!({}))).unwrap();
        // The client-side deadline expires while the handler is still running.
        // Abandon the connection instead of risking a late response being read
        // as the response to a future request.
        thread::sleep(Duration::from_millis(25));
        drop(stream);

        let mut second = connect(&name);
        assert_success(&handshake(&mut second, "h2"));
        server.shutdown().unwrap();
    }

    #[test]
    fn subscription_is_bounded_and_receives_shutdown_event() {
        let name = unique_pipe_name("events");
        let handler = Arc::new(MockHandler::new());
        let mut server = ProtocolServer::start(name.clone(), handler.clone()).unwrap();
        let mut stream = connect(&name);
        assert_success(&handshake(&mut stream, "h"));
        write_json_frame(
            &mut stream,
            &request("events", "subscribe_events", serde_json::json!({})),
        )
        .unwrap();
        assert_success(&read_json_frame(&mut stream).unwrap().unwrap());
        handler.events.publish(EventPayload::StatusChanged {
            state_revision: 7,
            suspended: true,
            capture_active: false,
        });
        let event: crate::EventEnvelope = read_json_frame(&mut stream).unwrap().unwrap();
        assert!(matches!(
            event.event,
            EventPayload::StatusChanged {
                state_revision: 7,
                ..
            }
        ));

        let (shutdown_tx, shutdown_rx) = mpsc::sync_channel(1);
        thread::spawn(move || {
            let result = server.shutdown();
            let _ = shutdown_tx.send(result);
        });
        let event: crate::EventEnvelope = read_json_frame(&mut stream).unwrap().unwrap();
        assert!(matches!(event.event, EventPayload::ServerShuttingDown));
        shutdown_rx
            .recv_timeout(Duration::from_secs(2))
            .unwrap()
            .unwrap();
    }
}
