use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct EffectId(u64);

impl EffectId {
    fn next(previous: u64) -> Option<(Self, u64)> {
        let value = previous.checked_add(1)?;
        Some((Self(value), value))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct AppGeneration(pub(crate) u64);

impl AppGeneration {
    pub(crate) const fn initial() -> Self {
        Self(1)
    }
}

impl Default for AppGeneration {
    fn default() -> Self {
        Self::initial()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct EffectContext {
    pub(crate) effect_id: EffectId,
    pub(crate) issued_generation: AppGeneration,
    pub(crate) session_id: Option<String>,
    pub(crate) request_digest: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum EffectKind {
    Exit,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct AppEffect {
    pub(crate) context: EffectContext,
    pub(crate) kind: EffectKind,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(dead_code)]
pub(crate) enum EffectFailure {
    Internal,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(dead_code)]
pub(crate) enum AppEffectOutcome {
    Completed,
    Failed(EffectFailure),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct AppEffectResult {
    pub(crate) context: EffectContext,
    pub(crate) kind: EffectKind,
    pub(crate) outcome: AppEffectOutcome,
}

#[derive(Debug, Default)]
pub(crate) struct EffectTracker {
    pub(crate) generation: AppGeneration,
    next_effect_id: u64,
    pub(crate) pending: BTreeMap<EffectId, AppEffect>,
}

impl EffectTracker {
    pub(crate) fn issue(
        &mut self,
        kind: EffectKind,
        session_id: Option<String>,
        request_digest: Option<String>,
    ) -> Option<AppEffect> {
        let (effect_id, next) = EffectId::next(self.next_effect_id)?;
        self.next_effect_id = next;
        let effect = AppEffect {
            context: EffectContext {
                effect_id,
                issued_generation: self.generation,
                session_id,
                request_digest,
            },
            kind,
        };
        self.pending.insert(effect_id, effect.clone());
        Some(effect)
    }

    pub(crate) fn finish(&mut self, result: &AppEffectResult) -> bool {
        let Some(effect) = self.pending.get(&result.context.effect_id) else {
            return false;
        };
        if effect.context != result.context || effect.kind != result.kind {
            return false;
        }
        self.pending.remove(&result.context.effect_id);
        true
    }
}
