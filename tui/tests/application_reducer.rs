#[path = "../src/application/mod.rs"]
mod application;
#[path = "../src/input/mod.rs"]
mod input;

use application::{
    reduce, AppAction, AppModel, BootState, ConnectionState, EffectResult, EffectValue,
    FocusTarget, Overlay, TerminalSize,
};

#[test]
fn boot_effects_are_ordered_correlated_and_complete_out_of_order() {
    let mut model = AppModel::default();
    let effects = reduce(&mut model, AppAction::Boot);
    assert_eq!(effects.len(), 4);
    assert_eq!(model.boot, BootState::Loading);
    assert!(effects.windows(2).all(|pair| pair[0].id < pair[1].id));

    let values = [
        EffectValue::SessionsLoaded { count: 3 },
        EffectValue::DefinitionsLoaded { count: 2 },
        EffectValue::PendingCommandLoaded,
        EffectValue::PreferencesLoaded,
    ];
    for (effect, value) in effects.iter().rev().zip(values) {
        reduce(
            &mut model,
            AppAction::EffectFinished(EffectResult {
                effect_id: effect.id,
                issued_generation: effect.issued_generation,
                value,
            }),
        );
    }
    assert_eq!(model.boot, BootState::Ready);
    assert_eq!(model.connection, ConnectionState::Online);
    assert_eq!((model.definition_count, model.session_count), (2, 3));
}

#[test]
fn foreign_or_mismatched_results_cannot_mutate_boot_state() {
    let mut model = AppModel::default();
    let effects = reduce(&mut model, AppAction::Boot);
    let definition = effects[2];
    reduce(
        &mut model,
        AppAction::EffectFinished(EffectResult {
            effect_id: definition.id,
            issued_generation: definition.issued_generation + 1,
            value: EffectValue::DefinitionsLoaded { count: 99 },
        }),
    );
    assert_eq!(model.definition_count, 0);
    assert_eq!(model.pending_effects.len(), 4);
    assert_eq!(model.stale_result_count, 1);
}

#[test]
fn blocking_overlays_own_focus_and_quit_requires_confirmation() {
    let mut model = AppModel::default();
    reduce(
        &mut model,
        AppAction::FocusChanged(FocusTarget::Conversation),
    );
    reduce(&mut model, AppAction::OverlayOpened(Overlay::Suspension));
    reduce(&mut model, AppAction::OverlayClosed);
    reduce(&mut model, AppAction::QuitRequested);
    assert_eq!(model.overlay, Some(Overlay::Suspension));
    assert!(reduce(&mut model, AppAction::QuitConfirmed).is_empty());

    model.overlay = None;
    reduce(&mut model, AppAction::QuitRequested);
    let exit = reduce(&mut model, AppAction::QuitConfirmed);
    assert!(model.quit_requested);
    assert_eq!(exit.len(), 1);
}

#[test]
fn every_terminal_size_is_representable_without_underflow() {
    let mut model = AppModel::default();
    for size in [
        TerminalSize {
            width: 0,
            height: 0,
        },
        TerminalSize {
            width: 19,
            height: 7,
        },
        TerminalSize {
            width: 20,
            height: 8,
        },
        TerminalSize {
            width: u16::MAX,
            height: u16::MAX,
        },
    ] {
        reduce(&mut model, AppAction::TerminalResized(size));
        assert_eq!(
            model.terminal_size.is_supported(),
            size.width >= 20 && size.height >= 8
        );
    }
}
