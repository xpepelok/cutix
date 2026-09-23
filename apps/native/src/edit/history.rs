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

pub(crate) struct HistoryEntry {
    pub(crate) label: &'static str,
    pub(crate) snapshot: Snapshot,
    pub(crate) scene_id: String,
    pub(crate) selection: Vec<String>,
    pub(crate) coalesce: Option<String>,
}

pub const MAX_UNDO_DEPTH: usize = 100;

#[derive(Default)]
pub struct History {
    pub(crate) undo_stack: std::collections::VecDeque<HistoryEntry>,
    pub(crate) redo_stack: std::collections::VecDeque<HistoryEntry>,
}

impl History {
    #[cfg(test)]
    pub fn depth(&self) -> (usize, usize) {
        (self.undo_stack.len(), self.redo_stack.len())
    }

    pub fn clear(&mut self) {
        self.undo_stack.clear();
        self.redo_stack.clear();
    }

    pub(crate) fn push(stack: &mut std::collections::VecDeque<HistoryEntry>, entry: HistoryEntry) {
        stack.push_back(entry);
        while stack.len() > MAX_UNDO_DEPTH {
            stack.pop_front();
        }
    }
}
