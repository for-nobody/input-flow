use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender, TryRecvError, TrySendError};
use std::sync::{Arc, Mutex};

use inputflow_config::Config;
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const PROTOCOL_VERSION: u32 = 1;
pub const MAX_FRAME_BYTES: usize = 1024 * 1024;
pub const MAX_REQUEST_ID_BYTES: usize = 128;
pub const MAX_REQUESTS_PER_CONNECTION: usize = 4096;
pub const MAX_CONNECTIONS: u32 = 8;
pub const MAX_SUBSCRIBERS: usize = 8;
pub const EVENT_QUEUE_CAPACITY: usize = 32;
pub const CLIENT_RESPONSE_TIMEOUT_MS: u64 = 3_000;
pub const CAPTURE_TIMEOUT_MIN_MS: u64 = 100;
pub const CAPTURE_TIMEOUT_MAX_MS: u64 = 30_000;

fn empty_object() -> Value {
    Value::Object(Default::default())
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RequestEnvelope {
    pub protocol_version: u32,
    pub request_id: String,
    pub method: String,
    #[serde(default = "empty_object")]
    pub params: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponseEnvelope {
    pub protocol_version: u32,
    pub request_id: String,
    #[serde(flatten)]
    pub body: ResponseBody,
}

impl ResponseEnvelope {
    pub fn success(request_id: impl Into<String>, result: Value) -> Self {
        Self {
            protocol_version: PROTOCOL_VERSION,
            request_id: request_id.into(),
            body: ResponseBody::Success { result },
        }
    }

    pub fn error(request_id: impl Into<String>, error: ErrorBody) -> Self {
        Self {
            protocol_version: PROTOCOL_VERSION,
            request_id: request_id.into(),
            body: ResponseBody::Error { error },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ResponseBody {
    Success { result: Value },
    Error { error: ErrorBody },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ErrorBody {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
}

impl ErrorBody {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            details: None,
        }
    }

    pub fn with_details(mut self, details: impl Into<String>) -> Self {
        self.details = Some(details.into());
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HandshakeParams {
    pub client_name: String,
    pub client_version: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HandshakeResult {
    pub server_name: String,
    pub server_version: String,
    pub protocol_version: u32,
    pub schema_version: u32,
    pub capabilities: Vec<String>,
    pub max_frame_bytes: usize,
    pub max_requests_per_connection: usize,
    pub event_queue_capacity: usize,
    pub client_response_timeout_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct EmptyParams {}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConfigParams {
    pub config: Config,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BeginCaptureParams {
    pub timeout_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CancelCaptureParams {
    pub session_id: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConfigResult {
    pub config: Config,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ValidationResult {
    pub valid: bool,
    pub rule_count: usize,
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PercentilesDto {
    pub samples: u64,
    pub p50: Option<u64>,
    pub p95: Option<u64>,
    pub p99: Option<u64>,
    pub max: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StatusResult {
    pub phase: String,
    pub suspended: bool,
    pub capture_active: bool,
    pub rule_count: usize,
    pub emergency_key: String,
    pub state_revision: u64,
    pub apply_reconciliation: String,
    pub reconciliation_request_id: Option<u64>,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StatsResult {
    pub observed_events: u64,
    pub output_batches_sent: u64,
    pub output_batches_failed: u64,
    pub output_batches_dropped: u64,
    pub callback_latency_us: Option<PercentilesDto>,
    pub hold_delay_us: Option<PercentilesDto>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PauseResult {
    pub suspended: bool,
    pub held_events: usize,
    pub requested_inputs: u32,
    pub inserted_inputs: u32,
    pub output_complete: bool,
    pub last_error: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResumeResult {
    pub suspended: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SaveStageDto {
    pub committed: bool,
    pub backup_created: bool,
    pub cleanup_warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeStageDto {
    pub replaced: bool,
    pub was_suspended: bool,
    pub rule_count: usize,
    pub emergency_key: String,
    pub pause: PauseResult,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApplyResult {
    pub outcome: String,
    pub rule_count: usize,
    pub validation_errors: Vec<String>,
    pub save: Option<SaveStageDto>,
    pub runtime: Option<RuntimeStageDto>,
    pub rollback: Option<SaveStageDto>,
    pub runtime_request_id: Option<u64>,
    pub error: Option<String>,
    pub recovery_required: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BeginCaptureResult {
    pub session_id: u64,
    pub timeout_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CancelCaptureResult {
    pub session_id: u64,
    pub cancelled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SubscriptionResult {
    pub subscription_id: u64,
    pub queue_capacity: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventEnvelope {
    pub protocol_version: u32,
    pub event_id: u64,
    #[serde(flatten)]
    pub event: EventPayload,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EventPayload {
    StatusChanged {
        state_revision: u64,
        suspended: bool,
        capture_active: bool,
    },
    ConfigApplied {
        outcome: String,
        rule_count: usize,
        recovery_required: bool,
    },
    CaptureCompleted {
        session_id: u64,
        outcome: CaptureOutcomeDto,
    },
    ServerShuttingDown,
    Heartbeat,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum CaptureOutcomeDto {
    Captured { input: CapturedInputDto },
    Cancelled,
    TimedOut,
    Shutdown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "input_type", rename_all = "snake_case")]
pub enum CapturedInputDto {
    Key {
        logical: String,
        physical: Option<PhysicalKeyDto>,
    },
    MouseButton {
        button: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PhysicalKeyDto {
    pub scan_code: u16,
    pub extended: bool,
}

struct Subscriber {
    sender: SyncSender<EventEnvelope>,
    sequence: Arc<Mutex<u64>>,
}

pub struct EventHub {
    next_subscriber_id: AtomicU64,
    subscribers: Mutex<Vec<Subscriber>>,
}

impl Default for EventHub {
    fn default() -> Self {
        Self::new()
    }
}

impl EventHub {
    pub const fn new() -> Self {
        Self {
            next_subscriber_id: AtomicU64::new(1),
            subscribers: Mutex::new(Vec::new()),
        }
    }

    pub fn subscribe(&self) -> Result<EventSubscription, ErrorBody> {
        let mut subscribers = self.subscribers.lock().map_err(|_| {
            ErrorBody::new("internal_error", "event subscriber registry is unavailable")
        })?;
        if subscribers.len() >= MAX_SUBSCRIBERS {
            return Err(ErrorBody::new(
                "subscriber_limit",
                "the maximum number of event subscribers is already connected",
            ));
        }
        let id = self.next_subscriber_id.fetch_add(1, Ordering::Relaxed);
        let sequence = Arc::new(Mutex::new(0));
        let (sender, receiver) = mpsc::sync_channel(EVENT_QUEUE_CAPACITY);
        subscribers.push(Subscriber {
            sender,
            sequence: Arc::clone(&sequence),
        });
        Ok(EventSubscription {
            id,
            receiver,
            sequence,
        })
    }

    pub fn publish(&self, payload: EventPayload) {
        let Ok(mut subscribers) = self.subscribers.lock() else {
            return;
        };
        subscribers.retain(|subscriber| {
            let Ok(mut sequence) = subscriber.sequence.lock() else {
                return false;
            };
            *sequence += 1;
            match subscriber.sender.try_send(EventEnvelope {
                protocol_version: PROTOCOL_VERSION,
                event_id: *sequence,
                event: payload.clone(),
            }) {
                Ok(()) | Err(TrySendError::Full(_)) => true,
                Err(TrySendError::Disconnected(_)) => false,
            }
        });
    }
}

pub struct EventSubscription {
    id: u64,
    receiver: Receiver<EventEnvelope>,
    sequence: Arc<Mutex<u64>>,
}

impl EventSubscription {
    pub const fn id(&self) -> u64 {
        self.id
    }

    pub(crate) fn receiver(&self) -> &Receiver<EventEnvelope> {
        &self.receiver
    }

    /// Recheck the queue while holding the same sequence lock used by
    /// publishers. This closes the timeout/publish race that could otherwise
    /// put a heartbeat with a newer ID on the wire before a queued event.
    pub(crate) fn heartbeat_or_pending(&self) -> Result<EventEnvelope, TryRecvError> {
        let mut sequence = self
            .sequence
            .lock()
            .map_err(|_| TryRecvError::Disconnected)?;
        match self.receiver.try_recv() {
            Ok(event) => Ok(event),
            Err(TryRecvError::Empty) => {
                *sequence += 1;
                Ok(EventEnvelope {
                    protocol_version: PROTOCOL_VERSION,
                    event_id: *sequence,
                    event: EventPayload::Heartbeat,
                })
            }
            Err(error) => Err(error),
        }
    }
}

pub enum HandlerReply {
    Response(Value),
    Subscription(EventSubscription),
}

pub fn to_value<T: Serialize>(value: T) -> Result<Value, ErrorBody> {
    serde_json::to_value(value).map_err(|error| {
        ErrorBody::new("internal_error", "failed to serialize the response")
            .with_details(error.to_string())
    })
}

pub fn parse_params<T>(params: Value) -> Result<T, ErrorBody>
where
    T: for<'de> Deserialize<'de>,
{
    if !params.is_object() {
        return Err(ErrorBody::new(
            "invalid_params",
            "request params must be a JSON object",
        ));
    }
    serde_json::from_value(params).map_err(|error| {
        ErrorBody::new("invalid_params", "request params are invalid")
            .with_details(error.to_string())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_hub_is_bounded_and_event_ids_expose_drops() {
        let hub = EventHub::new();
        let subscription = hub.subscribe().unwrap();
        for revision in 0..EVENT_QUEUE_CAPACITY as u64 + 3 {
            hub.publish(EventPayload::StatusChanged {
                state_revision: revision,
                suspended: false,
                capture_active: false,
            });
        }
        let first = subscription.receiver().recv().unwrap();
        assert_eq!(first.event_id, 1);
        while subscription.receiver().try_recv().is_ok() {}
        hub.publish(EventPayload::ServerShuttingDown);
        let after_drop = subscription.receiver().recv().unwrap();
        assert!(after_drop.event_id > EVENT_QUEUE_CAPACITY as u64);
    }

    #[test]
    fn strict_params_reject_unknown_fields() {
        let value = serde_json::json!({"unexpected": true});
        let error = parse_params::<EmptyParams>(value).unwrap_err();
        assert_eq!(error.code, "invalid_params");
    }

    #[test]
    fn rust_and_csharp_share_the_v1_golden_contract() {
        let handshake_text = include_str!("../../../fixtures/protocol/v1/handshake-request.json");
        let handshake: RequestEnvelope = serde_json::from_str(handshake_text).unwrap();
        assert_eq!(handshake.protocol_version, PROTOCOL_VERSION);
        assert_eq!(handshake.method, "handshake");

        let response_text = include_str!("../../../fixtures/protocol/v1/handshake-response.json");
        let response: ResponseEnvelope = serde_json::from_str(response_text).unwrap();
        assert!(matches!(response.body, ResponseBody::Success { .. }));

        let config_response_text =
            include_str!("../../../fixtures/protocol/v1/get-config-response.json");
        let config_response: ResponseEnvelope = serde_json::from_str(config_response_text).unwrap();
        let ResponseBody::Success { result } = config_response.body else {
            panic!("get-config golden must be successful");
        };
        let result: ConfigResult = serde_json::from_value(result).unwrap();
        let config_golden = inputflow_config::parse_and_migrate_json(include_str!(
            "../../../fixtures/config/v2-valid.json"
        ))
        .unwrap();
        assert_eq!(result.config, config_golden.config);

        let error_text = include_str!("../../../fixtures/protocol/v1/error-response.json");
        let error: ResponseEnvelope = serde_json::from_str(error_text).unwrap();
        assert!(matches!(
            error.body,
            ResponseBody::Error { error } if error.code == "duplicate_request_id"
        ));

        let event_text = include_str!("../../../fixtures/protocol/v1/capture-event.json");
        let event: EventEnvelope = serde_json::from_str(event_text).unwrap();
        assert!(matches!(
            event.event,
            EventPayload::CaptureCompleted { session_id: 4, .. }
        ));
    }
}
