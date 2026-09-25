use crate::{Debouncer, FileIdCache};
use file_id::FileId;
use notify::RecursiveMode;
use rustc_hash::FxHashMap as HashMap;
use std::path::{Path, PathBuf};

/// A cache to hold the file system IDs of all watched files.
///
/// The file ID cache uses unique file IDs provided by the file system and is used to stitch together
/// rename events in case the notification back-end doesn't emit rename cookies.
///
/// This type is a data container: the scanning, forgetting and rescan logic lives on
/// [`Debouncer`](crate::Debouncer), which coordinates file IDs together with the watch roots and
/// event queues.
#[derive(Debug, Clone, Default)]
pub struct FileIdMap {
    pub(crate) paths: HashMap<PathBuf, FileId>,
}

impl FileIdMap {
    /// Construct an empty cache.
    #[must_use]
    pub fn new() -> Self {
        Default::default()
    }
}

impl FileIdCache for FileIdMap {
    fn cached_file_id(&self, path: &Path) -> Option<impl AsRef<FileId>> {
        Debouncer::cached_file_id_of(self, path)
    }

    fn add_path(&mut self, path: &Path, recursive_mode: RecursiveMode) {
        Debouncer::scan_file_ids(self, path, recursive_mode);
    }

    fn remove_path(&mut self, path: &Path) {
        Debouncer::forget_file_ids(self, path);
    }

    fn rescan(&mut self, root_paths: &[(PathBuf, RecursiveMode)]) {
        Debouncer::rescan_file_ids(self, root_paths);
    }
}
