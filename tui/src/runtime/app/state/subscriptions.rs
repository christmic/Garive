use tokio::task::JoinHandle;

use crate::runtime::host::{LiveSubscriptionId, SubscriptionId};

use super::{BackgroundFollow, RuntimeState};

impl RuntimeState {
    pub(super) fn add_background_follow(
        &mut self,
        session_id: String,
        observed_position: u64,
        owner: SubscriptionId,
        task: JoinHandle<()>,
    ) {
        self.follow_sequence = self.follow_sequence.saturating_add(1);
        if let Some(mut replaced) = self.background_follows.remove(&session_id) {
            if let Some(previous) = replaced.follow.take() {
                previous.abort();
            }
            if let Some(previous) = replaced.reconnect.take() {
                previous.abort();
            }
        }
        self.background_follows.insert(
            session_id,
            BackgroundFollow {
                observed_position,
                attempt: 0,
                sequence: self.follow_sequence,
                follow: Some(task),
                follow_owner: Some(owner),
                reconnect: None,
                reconnect_owner: None,
            },
        );
        if self.background_follows.len() > 4 {
            let oldest = self
                .background_follows
                .iter()
                .min_by_key(|(_, value)| value.sequence)
                .map(|(key, _)| key.clone());
            if let Some(oldest) = oldest {
                if let Some(mut evicted) = self.background_follows.remove(&oldest) {
                    if let Some(task) = evicted.follow.take() {
                        task.abort();
                    }
                    if let Some(task) = evicted.reconnect.take() {
                        task.abort();
                    }
                }
            }
        }
    }

    #[cfg(test)]
    pub(in crate::runtime::app) fn add_background_follow_for_test(
        &mut self,
        session_id: String,
        observed_position: u64,
        owner: SubscriptionId,
        task: JoinHandle<()>,
    ) {
        self.add_background_follow(session_id, observed_position, owner, task);
    }

    pub(in crate::runtime::app) fn next_subscription_id(&mut self) -> SubscriptionId {
        self.subscription_sequence = self.subscription_sequence.saturating_add(1);
        SubscriptionId::new(self.subscription_sequence)
    }

    pub(in crate::runtime::app) fn next_live_subscription_id(&mut self) -> LiveSubscriptionId {
        self.live_subscription_sequence = self.live_subscription_sequence.saturating_add(1);
        LiveSubscriptionId::new(self.live_subscription_sequence)
    }

    pub(in crate::runtime::app) fn owns_subscription(
        &self,
        session_id: &str,
        subscription_id: SubscriptionId,
    ) -> bool {
        if self.model.selected_session.as_deref() == Some(session_id) {
            return self.follow.is_some() && self.follow_owner == Some(subscription_id);
        }
        self.background_follows
            .get(session_id)
            .is_some_and(|owner| {
                owner.follow.is_some() && owner.follow_owner == Some(subscription_id)
            })
    }

    #[cfg(test)]
    pub(in crate::runtime::app) fn background_follow_order(&self) -> Vec<String> {
        let mut follows = self
            .background_follows
            .iter()
            .map(|(session_id, follow)| (follow.sequence, session_id.clone()))
            .collect::<Vec<_>>();
        follows.sort_by_key(|(sequence, _)| *sequence);
        follows
            .into_iter()
            .map(|(_, session_id)| session_id)
            .collect()
    }
}
