//! The undo stack, and what a single undoable step is made of.

use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Retain {
    Both,
    Left,
    Right,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Edge {
    Start,
    End,
}

#[derive(Clone)]
pub(crate) enum Snapshot {
    Tracks(SceneTracks),
    Scenes { scenes: Vec<Scene>, current: String },
}

/// One undoable step: what it was called, what the document looked like before it, and
/// what was selected at the time.
///
/// The fields are open to `commands`, which is the only thing that builds or replays one.
pub(crate) struct HistoryEntry {
    /// The i18n key naming the step, so the interface can offer to undo it by name.
    pub(crate) label: &'static str,
    pub(crate) snapshot: Snapshot,
    pub(crate) scene_id: String,
    pub(crate) selection: Vec<String>,
    /// Steps sharing a key merge into one, so dragging a slider is a single undo rather
    /// than one per pixel.
    pub(crate) coalesce: Option<String>,
}

pub const MAX_UNDO_DEPTH: usize = 100;

/// The undo and redo stacks, bounded at [`MAX_UNDO_DEPTH`] steps each.
///
/// The stacks are open to `commands`, which owns the decision of what constitutes a step.
#[derive(Default)]
pub struct History {
    pub(crate) undo_stack: std::collections::VecDeque<HistoryEntry>,
    pub(crate) redo_stack: std::collections::VecDeque<HistoryEntry>,
}

impl History {
    /// Only the tests in this file ask for this; compiled for them alone so the
    /// shipping binary does not carry a method nothing calls.
    #[cfg(test)]
    pub fn depth(&self) -> (usize, usize) {
        (self.undo_stack.len(), self.redo_stack.len())
    }

    pub fn clear(&mut self) {
        self.undo_stack.clear();
        self.redo_stack.clear();
    }

    /// Pushes a step, dropping the oldest once the stack is at its depth.
    pub(crate) fn push(stack: &mut std::collections::VecDeque<HistoryEntry>, entry: HistoryEntry) {
        stack.push_back(entry);
        while stack.len() > MAX_UNDO_DEPTH {
            stack.pop_front();
        }
    }
}
