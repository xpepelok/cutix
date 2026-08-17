use std::collections::HashMap;
use std::path::{Path, PathBuf};

use cutix_project::MediaStore;

pub trait MediaResolver: Send {
    fn resolve(&self, media_id: &str) -> Option<PathBuf>;
}

#[derive(Debug, Default, Clone)]
pub struct MediaMap {
    entries: HashMap<String, PathBuf>,
}

impl MediaMap {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, media_id: impl Into<String>, path: impl Into<PathBuf>) -> &mut Self {
        self.entries.insert(media_id.into(), path.into());
        self
    }

    pub fn with(mut self, media_id: impl Into<String>, path: impl Into<PathBuf>) -> Self {
        self.insert(media_id, path);
        self
    }
}

impl MediaResolver for MediaMap {
    fn resolve(&self, media_id: &str) -> Option<PathBuf> {
        self.entries.get(media_id).cloned()
    }
}

pub struct StoreResolver {
    store: MediaStore,
}

impl StoreResolver {
    pub fn new(store: MediaStore) -> Self {
        Self { store }
    }

    pub fn root(&self) -> &Path {
        self.store.root()
    }
}

impl MediaResolver for StoreResolver {
    fn resolve(&self, media_id: &str) -> Option<PathBuf> {
        let asset = self.store.get(media_id).ok()?;
        let path = self.store.source_file(&asset);
        path.exists().then_some(path)
    }
}
