use garive_core::{assemble_model_inputs, derive_context};
use garive_eval::{
    summarize_context_pressure, ContextPressureCaseEvidence, ContextPressureSummary,
};

use crate::{
    ContextPressureCorpus, ContextPressureError, ContextPressureErrorCode, TokenCounter,
    TokenCounterDescriptor,
};

/// Complete deterministic non-secret result of one C7-A corpus run.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContextPressureRun {
    /// Exact corpus identity.
    pub corpus_id: String,
    /// Exact corpus revision.
    pub corpus_revision: String,
    /// Canonical corpus SHA-256.
    pub corpus_digest: String,
    /// Frozen counter identity/configuration.
    pub counter: TokenCounterDescriptor,
    /// Ordered case evidence and per-class reduction.
    pub summary: ContextPressureSummary,
}

/// Measures every uncompressed case through the sole injected counter route.
pub fn measure_context_pressure(
    corpus: &ContextPressureCorpus,
    counter: &dyn TokenCounter,
) -> Result<ContextPressureRun, ContextPressureError> {
    let descriptor = counter.descriptor().clone();
    if TokenCounterDescriptor::new(
        descriptor.counter_id.clone(),
        descriptor.counter_revision.clone(),
        descriptor.config_digest.clone(),
        descriptor.publishable,
    )
    .is_none()
    {
        return Err(error(ContextPressureErrorCode::InvalidCounter));
    }
    let mut evidence = Vec::with_capacity(corpus.cases.len());
    for case in &corpus.cases {
        let surface = derive_context(&case.request, &case.candidates)
            .map_err(|_| error(ContextPressureErrorCode::InvalidContext))?;
        if !surface.dropped_refs.is_empty() {
            return Err(error(ContextPressureErrorCode::CompressedInput));
        }
        let item_count = surface.item_count;
        let utf8_bytes = surface.utf8_bytes;
        let items = assemble_model_inputs(surface);
        let input_tokens = counter
            .count_input_tokens(&items)
            .map_err(|_| error(ContextPressureErrorCode::CounterFailure))?;
        evidence.push(
            ContextPressureCaseEvidence::new(
                case.case_id.clone(),
                case.workload_class,
                item_count,
                utf8_bytes,
                input_tokens,
                case.model_input_limit_tokens,
            )
            .map_err(|_| error(ContextPressureErrorCode::ReductionFailure))?,
        );
    }
    let summary = summarize_context_pressure(&evidence, corpus.cases.len())
        .map_err(|_| error(ContextPressureErrorCode::ReductionFailure))?;
    Ok(ContextPressureRun {
        corpus_id: corpus.corpus_id.clone(),
        corpus_revision: corpus.corpus_revision.clone(),
        corpus_digest: corpus.canonical_digest.clone(),
        counter: descriptor,
        summary,
    })
}

const fn error(code: ContextPressureErrorCode) -> ContextPressureError {
    ContextPressureError::new(code)
}
