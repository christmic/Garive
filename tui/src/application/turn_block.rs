use super::TimelineItem;
#[cfg(test)]
use super::{AppModel, TimelineRole, TimelineTone};

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct TurnBlockKey {
    pub(crate) session_id: String,
    pub(crate) turn_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct TurnBlock {
    pub(crate) key: TurnBlockKey,
    pub(crate) user: TimelineItem,
    pub(crate) activities: Vec<TimelineItem>,
    pub(crate) committed_answer: Option<TimelineItem>,
    pub(crate) outcome: Option<TimelineItem>,
}

impl TurnBlock {
    pub(crate) fn children(&self) -> impl Iterator<Item = &TimelineItem> {
        std::iter::once(&self.user)
            .chain(self.activities.iter())
            .chain(self.committed_answer.iter())
            .chain(self.outcome.iter())
    }

    pub(crate) fn child(&self, stable_key: &str) -> Option<&TimelineItem> {
        self.children().find(|child| child.stable_key == stable_key)
    }
}

#[cfg(test)]
impl AppModel {
    pub(crate) fn push_test_timeline_item(&mut self, item: TimelineItem) {
        match item.role {
            TimelineRole::User => self.turn_blocks.push(TurnBlock {
                key: TurnBlockKey {
                    session_id: "test-session".into(),
                    turn_id: item.stable_key.clone(),
                },
                user: item,
                activities: Vec::new(),
                committed_answer: None,
                outcome: None,
            }),
            TimelineRole::Status => {
                if let Some(block) = self.turn_blocks.last_mut() {
                    block.activities.push(item);
                }
            }
            TimelineRole::Agent => {
                if let Some(block) = self.turn_blocks.last_mut() {
                    block.committed_answer = Some(item);
                } else {
                    let key = item.stable_key.clone();
                    self.turn_blocks.push(TurnBlock {
                        key: TurnBlockKey {
                            session_id: "test-session".into(),
                            turn_id: key.clone(),
                        },
                        user: TimelineItem {
                            stable_key: format!("{key}:user"),
                            position: item.position,
                            role: TimelineRole::User,
                            tone: TimelineTone::Neutral,
                            text: String::new(),
                        },
                        activities: Vec::new(),
                        committed_answer: Some(item),
                        outcome: None,
                    });
                }
            }
        }
    }
}
