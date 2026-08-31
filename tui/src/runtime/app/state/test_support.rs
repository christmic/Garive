use garive_host_client::LiveHostClient;
use tokio::sync::mpsc;

use crate::persistence::{PendingCommand, Preferences, StateStore};

use super::{RestoredState, RuntimeState};

impl RuntimeState {
    pub(in crate::runtime) fn test_ephemeral(pending: Vec<PendingCommand>) -> Self {
        let config = crate::parse_launch_config([
            "garive-tui",
            "--host",
            "http://127.0.0.1:1",
            "--ephemeral",
        ])
        .expect("test launch config is valid");
        let client = LiveHostClient::new(&config.host, super::super::LIMITS)
            .expect("test Host URL is valid");
        let store = StateStore::open(None, true).expect("ephemeral state is available");
        let preferences = Preferences::default();
        let (sender, _receiver) = mpsc::channel(16);
        let (action_sender, _action_receiver) = mpsc::channel(16);
        Self::new(
            config,
            client,
            sender,
            action_sender,
            RestoredState {
                store,
                preferences: preferences.clone(),
                pending,
                pending_quarantined: 0,
                history: Vec::new(),
                history_error: false,
            },
        )
    }
}
