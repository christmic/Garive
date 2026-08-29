//! Bounded Runtime-owned buffering and neutral sink delivery for Agent signals.

use std::collections::VecDeque;

use garive_observability::{
    AgentSignal, Attribute, AttributeValue, Correlation, Measurement, MeasurementUnit,
    MeasurementValue, RedactionClass, Severity, SignalBinding,
};

/// Explicit non-environment Runtime limits for one observability buffer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ObservabilityLimits {
    /// Maximum queued signal count.
    pub max_signals: usize,
    /// Maximum canonical payload bytes.
    pub max_bytes: usize,
    /// Maximum signals offered in one sink call.
    pub flush_batch_size: usize,
    /// Exporter deadline supplied to an exporter implementation.
    pub flush_deadline_ms: u64,
    /// Info-and-lower sampling denominator; one admits every signal.
    pub sampling_denominator: u64,
    /// Independent attempts for an ordinary flush.
    pub exporter_retry_attempts: usize,
    /// Maximum sink calls made during shutdown.
    pub shutdown_flush_attempts: usize,
}
impl ObservabilityLimits {
    /// Rejects zero or internally impossible queue limits.
    pub const fn validate(self) -> Result<Self, ObservabilityRuntimeError> {
        if self.max_signals == 0
            || self.max_bytes == 0
            || self.flush_batch_size == 0
            || self.flush_deadline_ms == 0
            || self.sampling_denominator == 0
            || self.exporter_retry_attempts == 0
            || self.shutdown_flush_attempts == 0
        {
            Err(ObservabilityRuntimeError::InvalidConfiguration)
        } else {
            Ok(self)
        }
    }
}

/// Maximum source sensitivity admitted to this sink.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RedactionPolicy {
    /// Strongest source redaction class the sink accepts.
    pub maximum_class: RedactionClass,
}

/// Result returned by a neutral sink without changing Agent state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SinkDisposition {
    /// The complete offered prefix was accepted.
    Accepted,
    /// The sink requests a later retry without consuming the prefix.
    Backpressured,
    /// The sink is unavailable and consumed nothing.
    Unavailable,
}

/// Runtime-owned exporter boundary; configuration belongs to its constructor.
pub trait ObservabilitySink {
    /// Offers one canonical ordered queue prefix.
    fn emit(&mut self, batch: &[SignalBinding]) -> SinkDisposition;
}

/// Stable local enqueue outcome.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EnqueueDisposition {
    /// Signal was retained for export.
    Accepted,
    /// Deterministic low-priority sampling omitted the signal.
    SampledOut,
    /// Sink redaction policy omitted the signal.
    RedactionDropped,
    /// Queue pressure omitted the incoming signal.
    PressureDropped,
}

/// Runtime-only operational failure; it never becomes an Agent outcome.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ObservabilityRuntimeError {
    /// A zero or impossible explicit limit was supplied.
    InvalidConfiguration,
    /// Canonical signal serialization failed.
    SerializationFailure,
    /// A durability-derived signal does not match the committed position.
    InvalidDurablePosition,
    /// Drop-marker construction failed because Runtime supplied invalid time.
    InvalidDropSignal,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct QueueEntry {
    signal: AgentSignal,
    binding: SignalBinding,
    bytes: usize,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct DropStats {
    count: u64,
    bytes: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DropClass {
    Policy = 0,
    Pressure = 1,
    Sampling = 2,
}
impl DropClass {
    const ALL: [Self; 3] = [Self::Policy, Self::Pressure, Self::Sampling];
    const fn wire_name(self) -> &'static str {
        match self {
            Self::Policy => "policy",
            Self::Pressure => "pressure",
            Self::Sampling => "sampling",
        }
    }
}

/// Bounded deterministic signal buffer that has no durable Agent-state access.
#[derive(Debug)]
pub struct ObservabilityBuffer {
    limits: ObservabilityLimits,
    policy: RedactionPolicy,
    queue: VecDeque<QueueEntry>,
    queued_bytes: usize,
    pending_drops: [DropStats; 3],
}
impl ObservabilityBuffer {
    /// Constructs a buffer exclusively from explicit Garive configuration.
    pub fn new(
        limits: ObservabilityLimits,
        policy: RedactionPolicy,
    ) -> Result<Self, ObservabilityRuntimeError> {
        Ok(Self {
            limits: limits.validate()?,
            policy,
            queue: VecDeque::new(),
            queued_bytes: 0,
            pending_drops: [DropStats::default(); 3],
        })
    }

    /// Returns the retained signal count.
    pub fn queued_signals(&self) -> usize {
        self.queue.len()
    }

    /// Returns exact retained canonical bytes.
    pub const fn queued_bytes(&self) -> usize {
        self.queued_bytes
    }

    /// Returns the configured exporter deadline for sink implementations.
    pub const fn flush_deadline_ms(&self) -> u64 {
        self.limits.flush_deadline_ms
    }

    /// Enqueues a validated pre-commit or diagnostic signal.
    pub fn enqueue(
        &mut self,
        signal: AgentSignal,
    ) -> Result<EnqueueDisposition, ObservabilityRuntimeError> {
        let binding = signal
            .binding()
            .map_err(|_| ObservabilityRuntimeError::SerializationFailure)?;
        let bytes = binding.inline_utf8.len();
        if signal.redaction_class() > self.policy.maximum_class {
            self.record_drop(DropClass::Policy, bytes);
            return Ok(EnqueueDisposition::RedactionDropped);
        }
        if sampled_out(&signal, &binding.digest, self.limits.sampling_denominator) {
            self.record_drop(DropClass::Sampling, bytes);
            return Ok(EnqueueDisposition::SampledOut);
        }
        let incoming_priority = priority(&signal);
        while self.queue.len() >= self.limits.max_signals
            || self.queued_bytes.saturating_add(bytes) > self.limits.max_bytes
        {
            let Some(index) = self
                .queue
                .iter()
                .position(|entry| priority(&entry.signal) < incoming_priority)
            else {
                self.record_drop(DropClass::Pressure, bytes);
                return Ok(EnqueueDisposition::PressureDropped);
            };
            let evicted = self.queue.remove(index).expect("located queue entry");
            self.queued_bytes -= evicted.bytes;
            self.record_drop(DropClass::Pressure, evicted.bytes);
        }
        self.queued_bytes += bytes;
        self.queue.push_back(QueueEntry {
            signal,
            binding,
            bytes,
        });
        Ok(EnqueueDisposition::Accepted)
    }

    /// Enqueues only after a caller has committed the exact durable position.
    pub fn enqueue_committed(
        &mut self,
        signal: AgentSignal,
        committed_position: u64,
    ) -> Result<EnqueueDisposition, ObservabilityRuntimeError> {
        if committed_position == 0
            || signal.correlation().durable_position != Some(committed_position)
        {
            return Err(ObservabilityRuntimeError::InvalidDurablePosition);
        }
        self.enqueue(signal)
    }

    /// Offers a prefix with an independent bounded retry budget.
    pub fn flush<S: ObservabilitySink>(
        &mut self,
        sink: &mut S,
        observed_at_utc: &str,
    ) -> Result<SinkDisposition, ObservabilityRuntimeError> {
        self.materialize_drop_marker(observed_at_utc)?;
        let mut last = SinkDisposition::Accepted;
        for _ in 0..self.limits.exporter_retry_attempts {
            if self.queue.is_empty() {
                return Ok(SinkDisposition::Accepted);
            }
            let count = self.queue.len().min(self.limits.flush_batch_size);
            let batch: Vec<_> = self
                .queue
                .iter()
                .take(count)
                .map(|entry| entry.binding.clone())
                .collect();
            last = sink.emit(&batch);
            if last == SinkDisposition::Accepted {
                self.remove_prefix(count);
                return Ok(last);
            }
        }
        Ok(last)
    }

    /// Performs a bounded shutdown flush and reports telemetry abandoned locally.
    pub fn shutdown<S: ObservabilitySink>(
        &mut self,
        sink: &mut S,
        observed_at_utc: &str,
    ) -> Result<ShutdownReport, ObservabilityRuntimeError> {
        self.materialize_drop_marker(observed_at_utc)?;
        let mut attempts = 0;
        while !self.queue.is_empty() && attempts < self.limits.shutdown_flush_attempts {
            let count = self.queue.len().min(self.limits.flush_batch_size);
            let batch: Vec<_> = self
                .queue
                .iter()
                .take(count)
                .map(|entry| entry.binding.clone())
                .collect();
            attempts += 1;
            if sink.emit(&batch) == SinkDisposition::Accepted {
                self.remove_prefix(count);
            }
        }
        let report = ShutdownReport {
            attempts,
            abandoned_signals: self.queue.len(),
            abandoned_bytes: self.queued_bytes,
        };
        self.queue.clear();
        self.queued_bytes = 0;
        Ok(report)
    }

    fn materialize_drop_marker(
        &mut self,
        observed_at_utc: &str,
    ) -> Result<(), ObservabilityRuntimeError> {
        let pending = std::mem::take(&mut self.pending_drops);
        for class in DropClass::ALL {
            let stats = pending[class as usize];
            if stats.count != 0 {
                self.enqueue_drop_marker(observed_at_utc, class, stats)?;
            }
        }
        Ok(())
    }

    fn enqueue_drop_marker(
        &mut self,
        observed_at_utc: &str,
        class: DropClass,
        stats: DropStats,
    ) -> Result<(), ObservabilityRuntimeError> {
        let signal = AgentSignal::new(
            "agent.telemetry.dropped",
            1,
            observed_at_utc,
            Severity::Error,
            Correlation::default(),
            vec![Attribute {
                name: "classification".into(),
                value: AttributeValue::String {
                    value: class.wire_name().into(),
                },
            }],
            vec![
                Measurement {
                    name: "dropped_bytes".into(),
                    value: MeasurementValue::Known { value: stats.bytes },
                    unit: MeasurementUnit::Bytes,
                },
                Measurement {
                    name: "dropped_count".into(),
                    value: MeasurementValue::Known { value: stats.count },
                    unit: MeasurementUnit::Count,
                },
            ],
            RedactionClass::Operational,
        )
        .map_err(|_| ObservabilityRuntimeError::InvalidDropSignal)?;
        let _ = self.enqueue(signal)?;
        Ok(())
    }

    fn record_drop(&mut self, class: DropClass, bytes: usize) {
        let stats = &mut self.pending_drops[class as usize];
        stats.count = stats.count.saturating_add(1);
        stats.bytes = stats.bytes.saturating_add(bytes as u64);
    }

    fn remove_prefix(&mut self, count: usize) {
        for _ in 0..count {
            if let Some(entry) = self.queue.pop_front() {
                self.queued_bytes -= entry.bytes;
            }
        }
    }
}

/// Result of a bounded shutdown that never changes Agent state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ShutdownReport {
    /// Number of sink calls made.
    pub attempts: usize,
    /// Signals abandoned after the explicit bound.
    pub abandoned_signals: usize,
    /// Canonical bytes abandoned after the explicit bound.
    pub abandoned_bytes: usize,
}

fn priority(signal: &AgentSignal) -> Severity {
    if signal.signal_name() == "agent.telemetry.dropped" {
        Severity::Error
    } else {
        signal.severity()
    }
}

fn sampled_out(signal: &AgentSignal, digest: &str, denominator: u64) -> bool {
    if denominator == 1 || priority(signal) >= Severity::Warn {
        return false;
    }
    let prefix = u64::from_str_radix(&digest[..16], 16).expect("SHA-256 prefix");
    prefix % denominator != 0
}
