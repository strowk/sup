use crate::sup::SupState;
use serde::{Deserialize, Serialize};

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub(crate) enum SupStateSerde {
    Idle,
    InProgress(bool, String, Option<String>),
    Interrupted(bool, String, Option<String>, bool),
}

impl From<SupState> for SupStateSerde {
    fn from(state: SupState) -> Self {
        match state {
            SupState::Idle => SupStateSerde::Idle,
            SupState::InProgress {
                stash_created,
                original_head,
                message,
            } => SupStateSerde::InProgress(stash_created, original_head, message),
            SupState::Interrupted {
                stash_created,
                original_head,
                message,
                stash_applied,
            } => SupStateSerde::Interrupted(stash_created, original_head, message, stash_applied),
        }
    }
}

impl From<SupStateSerde> for SupState {
    fn from(state: SupStateSerde) -> Self {
        match state {
            SupStateSerde::Idle => SupState::Idle,
            SupStateSerde::InProgress(stash_created, original_head, message) => {
                SupState::InProgress {
                    stash_created,
                    original_head,
                    message,
                }
            }
            SupStateSerde::Interrupted(stash_created, original_head, message, stash_applied) => {
                SupState::Interrupted {
                    stash_created,
                    original_head,
                    message,
                    stash_applied,
                }
            }
        }
    }
}
