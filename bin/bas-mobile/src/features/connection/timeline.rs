use super::{Message, state};
use std::ops::Range;

#[derive(Clone, PartialEq)]
pub(crate) struct Row {
    pub key: String,
    pub items: Range<usize>,
    pub activity: bool,
    turn: u64,
}
impl Message {
    pub fn activity(&self) -> bool {
        self.tool.is_some() || self.role == "thought"
    }
}

// GDS Agents groups adjacent activity within a turn; prose is always a boundary.
pub(crate) fn rows() -> Vec<Row> {
    state().with(|s| {
        let mut rows: Vec<Row> = Vec::new();
        for (index, message) in s.messages.iter().enumerate() {
            if message.activity()
                && let Some(last) = rows.last_mut()
                && last.activity
                && last.turn == message.turn
            {
                last.items.end = index + 1;
            } else {
                rows.push(Row {
                    key: format!(
                        "{}:{}",
                        s.session.as_deref().unwrap_or_default(),
                        message.sequence
                    ),
                    items: index..index + 1,
                    activity: message.activity(),
                    turn: message.turn,
                });
            }
        }
        rows
    })
}
pub(crate) fn messages(row: &Row) -> Vec<Message> {
    state().with(|s| {
        s.messages
            .get(row.items.clone())
            .unwrap_or_default()
            .to_vec()
    })
}
