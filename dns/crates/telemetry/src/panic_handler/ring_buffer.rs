use crate::error::{TelemetryError, TelemetryResult};
use std::collections::VecDeque;
use std::fmt::Display;
use std::sync::{Arc, Mutex, OnceLock};
use time::OffsetDateTime;
use tracing::{Event, Subscriber};
use tracing_subscriber::Layer;
use tracing_subscriber::layer::Context;
use tracing_subscriber::registry::LookupSpan;

pub(crate) static RECENT_LOGS: OnceLock<Arc<Mutex<BoundedRingBuffer<CapturedLog>>>> = OnceLock::new();

#[derive(Clone)]
pub struct RingBufferLayer;

impl RingBufferLayer {
    pub fn init(buffer: BoundedRingBuffer<CapturedLog>) -> Self {
        let _ = RECENT_LOGS.set(Arc::new(Mutex::new(buffer)));
        
        Self {}
    }

    pub fn flush(&self) -> TelemetryResult<Vec<CapturedLog>> {
        if let Some(buffer) = RECENT_LOGS.get() {
            if let Ok(mut buffer) = buffer.lock() {
                let logs = buffer.iter().cloned().collect();
                buffer.inner.clear();
                
                Ok(logs)
            } else {
                Err(TelemetryError::panic_hook("Failed to acquire lock on log buffer".to_string()))
            }
        } else {
            Err(TelemetryError::panic_hook("Log buffer not initialized".to_string()))
        }
    }
}

impl<S> Layer<S> for RingBufferLayer
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        let metadata = event.metadata();

        let mut message = String::new();
        let mut visitor = MessageVisitor(&mut message);
        event.record(&mut visitor);

        let log = CapturedLog {
            timestamp: OffsetDateTime::now_utc(),
            level: *metadata.level(),
            target: metadata.target().to_string(),
            message,
        };

        if let Some(buffer) = RECENT_LOGS.get() && let Ok(mut buffer) = buffer.lock() {
            buffer.push(log);
        }
    }
}

struct MessageVisitor<'a>(&'a mut String);

impl<'a> tracing::field::Visit for MessageVisitor<'a> {
    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        if field.name() == "message" {
            self.0.push_str(value);
        } else {
            self.0.push_str(&format!(" {}={value}", field.name()));
        }
    }

    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            self.0.push_str(&format!("{value:?}"));
        } else {
            self.0.push_str(&format!(" {}={value:?}", field.name()));
        }
    }
}

pub struct BoundedRingBuffer<T> {
    inner: VecDeque<T>,
    capacity: usize,
}

impl<T> BoundedRingBuffer<T> {
    pub const fn new(capacity: usize) -> Self {
        Self { inner: VecDeque::new(), capacity }
    }

    pub fn push(&mut self, item: T) {
        if self.inner.len() == self.capacity {
            self.inner.pop_front();
        }
        
        self.inner.push_back(item);
    }

    pub fn iter(&self) -> impl Iterator<Item = &T> {
        self.inner.iter()
    }

    #[allow(dead_code)]
    pub fn len(&self) -> usize {
        self.inner.len()
    }
}

#[derive(Clone, Debug)]
pub struct CapturedLog {
    pub timestamp: OffsetDateTime,
    pub level: tracing::Level,
    pub target: String,
    pub message: String,
}

impl Display for CapturedLog {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}] {} {} - {}", self.timestamp, self.level, self.target, self.message)
    }
}
