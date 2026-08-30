#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct EffectId(pub(crate) u64);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum EffectKind {
    LoadPreferences,
    LoadPendingCommand,
    LoadDefinitions,
    LoadSessions,
    Exit,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct AppEffect {
    pub(crate) id: EffectId,
    pub(crate) issued_generation: u64,
    pub(crate) kind: EffectKind,
}
