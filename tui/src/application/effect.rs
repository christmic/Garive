#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum EffectKind {
    Exit,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct AppEffect {
    pub(crate) kind: EffectKind,
}
