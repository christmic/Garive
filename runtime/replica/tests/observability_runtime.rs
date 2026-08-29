use garive_observability::{
    AgentSignal, Attribute, AttributeValue, Correlation, Measurement, MeasurementUnit,
    MeasurementValue, RedactionClass, Severity, SignalBinding,
};
use garive_runtime::{
    EnqueueDisposition, ObservabilityBuffer, ObservabilityLimits, ObservabilityRuntimeError,
    ObservabilitySink, RedactionPolicy, SinkDisposition,
};

#[derive(Default)]
struct FakeSink {
    dispositions: Vec<SinkDisposition>,
    calls: Vec<Vec<SignalBinding>>,
}
impl FakeSink {
    fn with(dispositions: Vec<SinkDisposition>) -> Self {
        Self {
            dispositions,
            calls: Vec::new(),
        }
    }
}
impl ObservabilitySink for FakeSink {
    fn emit(&mut self, batch: &[SignalBinding]) -> SinkDisposition {
        self.calls.push(batch.to_vec());
        if self.dispositions.is_empty() {
            SinkDisposition::Accepted
        } else {
            self.dispositions.remove(0)
        }
    }
}

fn limits() -> ObservabilityLimits {
    ObservabilityLimits {
        max_signals: 8,
        max_bytes: 16_384,
        flush_batch_size: 8,
        flush_deadline_ms: 100,
        sampling_denominator: 1,
        exporter_retry_attempts: 2,
        shutdown_flush_attempts: 2,
    }
}

fn buffer(limits: ObservabilityLimits, maximum_class: RedactionClass) -> ObservabilityBuffer {
    ObservabilityBuffer::new(limits, RedactionPolicy { maximum_class }).expect("valid buffer")
}

fn execution_signal(severity: Severity, position: Option<u64>) -> AgentSignal {
    AgentSignal::new(
        "agent.execution.terminal",
        1,
        "2026-08-29T00:00:00Z",
        severity,
        Correlation {
            trace_id: Some("11111111111111111111111111111111".into()),
            span_id: Some("2222222222222222".into()),
            execution_id: Some("execution".into()),
            durable_position: position,
            ..Correlation::default()
        },
        vec![
            Attribute {
                name: "outcome".into(),
                value: AttributeValue::String {
                    value: "completed".into(),
                },
            },
            Attribute {
                name: "replayed".into(),
                value: AttributeValue::Bool { value: false },
            },
            Attribute {
                name: "success".into(),
                value: AttributeValue::Bool { value: true },
            },
        ],
        vec![Measurement {
            name: "completed_iterations".into(),
            value: MeasurementValue::Known { value: 1 },
            unit: MeasurementUnit::Count,
        }],
        RedactionClass::Operational,
    )
    .expect("execution signal")
}

fn interaction_signal(secret: &str) -> AgentSignal {
    AgentSignal::new(
        "agent.interaction.required",
        1,
        "2026-08-29T00:00:01Z",
        Severity::Warn,
        Correlation {
            session_id: Some(secret.into()),
            ..Correlation::default()
        },
        vec![Attribute {
            name: "classification".into(),
            value: AttributeValue::String {
                value: "approval".into(),
            },
        }],
        vec![Measurement {
            name: "item_count".into(),
            value: MeasurementValue::Known { value: 1 },
            unit: MeasurementUnit::Count,
        }],
        RedactionClass::Restricted,
    )
    .expect("interaction signal")
}

#[test]
fn explicit_limits_and_committed_positions_fail_closed() {
    let mut invalid = limits();
    invalid.sampling_denominator = 0;
    assert_eq!(
        ObservabilityBuffer::new(
            invalid,
            RedactionPolicy {
                maximum_class: RedactionClass::Operational,
            }
        )
        .expect_err("zero denominator"),
        ObservabilityRuntimeError::InvalidConfiguration
    );

    let mut buffer = buffer(limits(), RedactionClass::Operational);
    assert_eq!(
        buffer.enqueue_committed(execution_signal(Severity::Info, Some(7)), 6),
        Err(ObservabilityRuntimeError::InvalidDurablePosition)
    );
    assert_eq!(buffer.queued_signals(), 0);
    assert_eq!(
        buffer.enqueue_committed(execution_signal(Severity::Info, Some(7)), 7),
        Ok(EnqueueDisposition::Accepted)
    );
}

#[test]
fn replay_duplicates_preserve_order_and_backpressure_consumes_nothing() {
    let mut buffer = buffer(limits(), RedactionClass::Operational);
    let signal = execution_signal(Severity::Info, Some(7));
    buffer
        .enqueue_committed(signal.clone(), 7)
        .expect("first replay");
    buffer.enqueue_committed(signal, 7).expect("second replay");
    let before = buffer.queued_bytes();
    let mut sink = FakeSink::with(vec![
        SinkDisposition::Backpressured,
        SinkDisposition::Unavailable,
    ]);
    assert_eq!(
        buffer.flush(&mut sink, "2026-08-29T00:00:02Z"),
        Ok(SinkDisposition::Unavailable)
    );
    assert_eq!(buffer.queued_signals(), 2);
    assert_eq!(buffer.queued_bytes(), before);
    assert_eq!(sink.calls.len(), 2);
    assert_eq!(sink.calls[0], sink.calls[1]);
    assert_eq!(sink.calls[0][0], sink.calls[0][1]);

    assert_eq!(
        buffer.flush(&mut sink, "2026-08-29T00:00:03Z"),
        Ok(SinkDisposition::Accepted)
    );
    assert_eq!(buffer.queued_signals(), 0);
}

#[test]
fn pressure_evicts_only_older_lower_priority_and_reports_drops() {
    let mut configured = limits();
    configured.max_signals = 2;
    let mut buffer = buffer(configured, RedactionClass::Operational);
    buffer
        .enqueue(execution_signal(Severity::Info, None))
        .expect("first info");
    buffer
        .enqueue(execution_signal(Severity::Info, None))
        .expect("second info");
    assert_eq!(
        buffer.enqueue(execution_signal(Severity::Warn, None)),
        Ok(EnqueueDisposition::Accepted)
    );
    assert_eq!(buffer.queued_signals(), 2);

    let mut sink = FakeSink::default();
    buffer
        .flush(&mut sink, "2026-08-29T00:00:04Z")
        .expect("flush");
    let payloads: Vec<_> = sink.calls[0]
        .iter()
        .map(|binding| binding.inline_utf8.as_str())
        .collect();
    assert!(payloads[0].contains("\"severity\":\"warn\""));
    assert!(payloads[1].contains("agent.telemetry.dropped"));
    assert!(payloads[1].contains("\"classification\""));
    assert!(payloads[1].contains("\"pressure\""));
}

#[test]
fn sampling_is_deterministic_and_never_omits_warn() {
    let mut configured = limits();
    configured.sampling_denominator = u64::MAX;
    let mut left = buffer(configured, RedactionClass::Operational);
    let mut right = buffer(configured, RedactionClass::Operational);
    let signal = execution_signal(Severity::Info, None);
    let left_outcome = left.enqueue(signal.clone()).expect("left sampling");
    let right_outcome = right.enqueue(signal).expect("right sampling");
    assert_eq!(left_outcome, right_outcome);
    assert_eq!(left_outcome, EnqueueDisposition::SampledOut);
    assert_eq!(
        left.enqueue(execution_signal(Severity::Warn, None)),
        Ok(EnqueueDisposition::Accepted)
    );
}

#[test]
fn restricted_secret_is_absent_from_debug_and_export_payloads() {
    const SECRET: &str = "SECRET_CANARY_DO_NOT_EXPORT";
    let mut buffer = buffer(limits(), RedactionClass::Operational);
    assert_eq!(
        buffer.enqueue(interaction_signal(SECRET)),
        Ok(EnqueueDisposition::RedactionDropped)
    );
    assert!(!format!("{buffer:?}").contains(SECRET));
    let mut sink = FakeSink::default();
    buffer
        .flush(&mut sink, "2026-08-29T00:00:05Z")
        .expect("drop marker flush");
    assert!(sink.calls[0][0].inline_utf8.contains("\"policy\""));
    assert!(!sink.calls[0][0].inline_utf8.contains(SECRET));
}

#[test]
fn shutdown_is_bounded_and_clears_only_telemetry() {
    let mut buffer = buffer(limits(), RedactionClass::Operational);
    buffer
        .enqueue(execution_signal(Severity::Error, None))
        .expect("signal");
    let mut sink = FakeSink::with(vec![
        SinkDisposition::Unavailable,
        SinkDisposition::Backpressured,
    ]);
    let report = buffer
        .shutdown(&mut sink, "2026-08-29T00:00:06Z")
        .expect("bounded shutdown");
    assert_eq!(report.attempts, 2);
    assert_eq!(report.abandoned_signals, 1);
    assert!(report.abandoned_bytes > 0);
    assert_eq!(buffer.queued_signals(), 0);
    assert_eq!(buffer.queued_bytes(), 0);
}
