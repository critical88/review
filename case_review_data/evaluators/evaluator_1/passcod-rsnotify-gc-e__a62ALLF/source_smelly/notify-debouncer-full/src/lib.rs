//! A debouncer for [notify] that is optimized for ease of use.
//!
//! * Only emits a single `Rename` event if the rename `From` and `To` events can be matched
//! * Merges multiple `Rename` events
//! * Takes `Rename` events into account and updates paths for events that occurred before the rename event, but which haven't been emitted, yet
//! * Optionally keeps track of the file system IDs all files and stitches rename events together (macOS FS Events, Windows)
//! * Emits only one `Remove` event when deleting a directory (inotify)
//! * Doesn't emit duplicate create events
//! * Doesn't emit `Modify` events after a `Create` event
//!
//! # Installation
//!
//! ```toml
//! [dependencies]
//! notify-debouncer-full = "0.8.0-rc.2"
//! ```
//!
//! In case you want to select specific features of notify,
//! specify notify as dependency explicitly in your dependencies.
//! Otherwise you can just use the re-export of notify from debouncer-full.
//!
//! ```toml
//! notify-debouncer-full = "0.8.0-rc.2"
//! notify = { version = "..", features = [".."] }
//! ```
//!
//! # Examples
//!
//! ```rust,no_run
//! # use std::path::Path;
//! # use std::time::Duration;
//! use notify_debouncer_full::{notify::*, new_debouncer, DebounceEventResult};
//!
//! // Select recommended watcher for debouncer.
//! // Using a callback here, could also be a channel.
//! let mut debouncer = new_debouncer(Duration::from_secs(2), None, |result: DebounceEventResult| {
//!     match result {
//!         Ok(events) => events.iter().for_each(|event| println!("{event:?}")),
//!         Err(errors) => errors.iter().for_each(|error| println!("{error:?}")),
//!     }
//! }).unwrap();
//!
//! // Add a path to be watched. All files and directories at that path and
//! // below will be monitored for changes.
//! debouncer.watch(".", RecursiveMode::Recursive).unwrap();
//! ```
//!
//! # Features
//!
//! The following crate features can be configured in your Cargo dependency:
//!
//! - `macos_fsevent` (default) enables notify's FSEvents backend on macOS
//! - `freebsd_inotify` enables notify's inotify backend on FreeBSD 14.5+
//!   - inotify is automatically enabled when built natively, this feature is only needed for cross-compilation
//! - `macos_kqueue` enables notify's kqueue backend on macOS
//! - `serde` enables serialization support in notify-types
//! - `web-time` uses `web_time::Instant` for debounced events
//! - `crossbeam-channel`, `flume`, `futures`, and `tokio` enable the corresponding
//!   channel senders as event handlers
//! - `serialization-compat-6` restores notify 6 serialization behavior
//!
//! # Caveats
//!
//! As all file events are sourced from notify, the [known problems](https://docs.rs/notify/latest/notify/#known-problems) section applies here too.
//!
//! # Architecture
//!
//! [`Debouncer`] is the single coordinator of the debouncing pipeline: it owns the debounced
//! event queues, the rename and rescan bookkeeping, the watch roots, the file ID scanning, and
//! the wiring of the watcher and the delivery thread. The state it works on is a plain data
//! record ([`DebounceDataInner`]); every policy decision lives on the coordinator itself.

mod cache;
#[cfg(test)]
mod time;

#[cfg(test)]
mod testing;

#[cfg(not(target_family = "wasm"))]
mod file_id_map;

use std::{
    cmp::Reverse,
    collections::{BinaryHeap, VecDeque},
    path::{Path, PathBuf},
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use rustc_hash::FxHashMap as HashMap;

pub use cache::{FileIdCache, NoCache, RecommendedCache};

#[cfg(not(target_family = "wasm"))]
pub use file_id_map::FileIdMap;

pub use file_id;
pub use notify;
pub use notify_types::debouncer_full::DebouncedEvent;

use file_id::FileId;
#[cfg(not(target_family = "wasm"))]
use file_id::get_file_id;
use notify::{
    Error, ErrorKind, Event, EventKind, PathOp, RecommendedWatcher, RecursiveMode,
    UpdatePathsError, Watcher, WatcherKind,
    event::{ModifyKind, RemoveKind, RenameMode},
};
#[cfg(not(target_family = "wasm"))]
use walkdir::WalkDir;

/// The set of requirements for watcher debounce event handling functions.
///
/// # Example implementation
///
/// ```rust,no_run
/// # use notify::{Event, Result, EventHandler};
/// # use notify_debouncer_full::{DebounceEventHandler, DebounceEventResult};
///
/// /// Prints received events
/// struct EventPrinter;
///
/// impl DebounceEventHandler for EventPrinter {
///     fn handle_event(&mut self, result: DebounceEventResult) {
///         match result {
///             Ok(events) => events.iter().for_each(|event| println!("{event:?}")),
///             Err(errors) => errors.iter().for_each(|error| println!("{error:?}")),
///         }
///     }
/// }
/// ```
pub trait DebounceEventHandler: Send + 'static {
    /// Handles an event.
    fn handle_event(&mut self, event: DebounceEventResult);
}

impl<F> DebounceEventHandler for F
where
    F: FnMut(DebounceEventResult) + Send + 'static,
{
    fn handle_event(&mut self, event: DebounceEventResult) {
        (self)(event);
    }
}

#[cfg(feature = "crossbeam-channel")]
impl DebounceEventHandler for crossbeam_channel::Sender<DebounceEventResult> {
    fn handle_event(&mut self, event: DebounceEventResult) {
        let _ = self.send(event);
    }
}

#[cfg(feature = "flume")]
impl DebounceEventHandler for flume::Sender<DebounceEventResult> {
    fn handle_event(&mut self, event: DebounceEventResult) {
        let _ = self.send(event);
    }
}

#[cfg(feature = "futures")]
impl DebounceEventHandler for futures::channel::mpsc::UnboundedSender<DebounceEventResult> {
    fn handle_event(&mut self, event: DebounceEventResult) {
        let _ = self.unbounded_send(event);
    }
}

#[cfg(feature = "tokio")]
impl DebounceEventHandler for tokio::sync::mpsc::UnboundedSender<DebounceEventResult> {
    fn handle_event(&mut self, event: DebounceEventResult) {
        let _ = self.send(event);
    }
}

impl DebounceEventHandler for std::sync::mpsc::Sender<DebounceEventResult> {
    fn handle_event(&mut self, event: DebounceEventResult) {
        let _ = self.send(event);
    }
}

/// A result of debounced events.
/// Comes with either a vec of events or vec of errors.
pub type DebounceEventResult = Result<Vec<DebouncedEvent>, Vec<Error>>;

type DebounceData = Arc<SharedDebounceData>;

#[derive(Debug)]
struct SharedDebounceData {
    inner: Mutex<DebounceDataInner>,
    changed: Condvar,
}

/// The per-path queues the debouncer stores pending debounced events in.
///
/// Events must be stored in the following order:
/// 1. `remove` or `move out` event
/// 2. `rename` event
/// 3. Other events
///
/// The queue itself is inert data; the ordering rules are enforced by
/// [`Debouncer`], which owns the debouncing policy.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct Queue {
    events: VecDeque<DebouncedEvent>,
}

/// The debouncer state: pending queues, watch roots, in-flight rename events
/// and pending errors.
///
/// This is a plain data record. All behavior operating on it lives on
/// [`Debouncer`], so every debouncing decision is in one place; the watcher
/// thread and the delivery thread both hand the record to the coordinator.
#[derive(Debug)]
pub(crate) struct DebounceDataInner {
    pub(crate) queues: HashMap<PathBuf, Queue>,
    /// Registered watch roots, kept **sorted by path** so that the coordinator's
    /// `add_root` can dedupe via binary search in O(log N) and doesn't suffer
    /// from injection
    pub(crate) roots: VecDeque<(PathBuf, RecursiveMode)>,
    pub(crate) rename_event: Option<(DebouncedEvent, Option<FileId>)>,
    pub(crate) rescan_event: Option<DebouncedEvent>,
    pub(crate) errors: Vec<Error>,
    pub(crate) timeout: Duration,
}

/// Debouncer guard, stops the debouncer on drop.
///
/// `Debouncer` is the one object that owns the whole debouncing pipeline:
/// the state record the watcher thread writes into, the file ID cache, the
/// queue policy, the expiry logic, the watch roots and the delivery thread.
///
/// The state-handling and file-ID-scanning logic is exposed as associated
/// functions so the watcher thread and the delivery thread can execute the
/// full pipeline without holding a debouncer guard themselves.
#[derive(Debug)]
pub struct Debouncer<T: Watcher, C: FileIdCache> {
    watcher: T,
    debouncer_thread: Option<std::thread::JoinHandle<()>>,
    data: DebounceData,
    file_id_cache: Arc<Mutex<C>>,
    stop: Arc<AtomicBool>,
}

impl<T: Watcher, C: FileIdCache> Debouncer<T, C> {
    // ===== lifecycle =====================================================

    /// Create a new debounced watcher with custom configuration.
    ///
    /// Timeout is the amount of time after which a debounced event is emitted.
    ///
    /// If `tick_rate` is `None`, notify will select a tick rate that is 1/4 of the provided timeout.
    ///
    /// The debouncer wires the three moving parts together: the watcher thread ingesting raw
    /// events, the pending-event state, and the delivery thread that flushes expired buckets.
    pub fn new_opt<F: DebounceEventHandler>(
        timeout: Duration,
        tick_rate: Option<Duration>,
        mut event_handler: F,
        file_id_cache: C,
        config: notify::Config,
    ) -> Result<Self, Error>
    where
        C: Send + 'static,
    {
        let data = Arc::new(SharedDebounceData {
            inner: Mutex::new(DebounceDataInner {
                queues: HashMap::default(),
                roots: VecDeque::new(),
                rename_event: None,
                rescan_event: None,
                errors: Vec::new(),
                timeout,
            }),
            changed: Condvar::new(),
        });
        let file_id_cache = Arc::new(Mutex::new(file_id_cache));
        let stop = Arc::new(AtomicBool::new(false));

        let tick_div = 4;
        let tick = match tick_rate {
            Some(v) => {
                if v > timeout {
                    return Err(Error::new(ErrorKind::Generic(format!(
                        "Invalid tick_rate, tick rate {v:?} > {timeout:?} timeout!"
                    ))));
                }
                v
            }
            None => timeout.checked_div(tick_div).ok_or_else(|| {
                Error::new(ErrorKind::Generic(format!(
                    "Failed to calculate tick as {timeout:?}/{tick_div}!"
                )))
            })?,
        };

        let data_c = data.clone();
        let stop_c = stop.clone();
        let thread = std::thread::Builder::new()
            .name("notify-rs debouncer loop".to_string())
            .spawn(move || {
                loop {
                    let mut lock = data_c.inner.lock().unwrap();
                    while lock.queues.is_empty()
                        && lock.errors.is_empty()
                        && lock.rescan_event.is_none()
                        && !stop_c.load(Ordering::Acquire)
                    {
                        lock = data_c.changed.wait(lock).unwrap();
                    }
                    if stop_c.load(Ordering::Acquire) {
                        break;
                    }
                    drop(lock);
                    std::thread::sleep(tick);
                    if stop_c.load(Ordering::Acquire) {
                        break;
                    }
                    lock = data_c.inner.lock().unwrap();
                    let send_data = Self::expire_events(&mut lock);
                    let errors = Self::take_errors(&mut lock);
                    drop(lock);
                    if !send_data.is_empty() {
                        event_handler.handle_event(Ok(send_data));
                    }
                    if !errors.is_empty() {
                        event_handler.handle_event(Err(errors));
                    }
                }
            })?;

        let data_c = data.clone();
        let file_id_cache_c = file_id_cache.clone();
        let watcher = T::new(
            move |e: Result<Event, Error>| {
                let mut lock = data_c.inner.lock().unwrap();
                let mut file_id_cache_guard = file_id_cache_c.lock().unwrap();
                match e {
                    Ok(e) => Self::dispatch_event(&mut lock, &mut *file_id_cache_guard, e),
                    // can't have multiple TX, so we need to pipe that through our debouncer
                    Err(e) => Self::push_error(&mut lock, e),
                }
                drop(file_id_cache_guard);
                drop(lock);
                data_c.changed.notify_all();
            },
            config,
        )?;

        let guard = Debouncer {
            watcher,
            debouncer_thread: Some(thread),
            data,
            file_id_cache,
            stop,
        };

        Ok(guard)
    }

    /// Stop the debouncer, waits for the event thread to finish.
    /// May block for the duration of one `tick_rate`.
    pub fn stop(mut self) {
        self.set_stop();
        if let Some(t) = self.debouncer_thread.take() {
            let _ = t.join();
        }
    }

    /// Stop the debouncer, does not wait for the event thread to finish.
    pub fn stop_nonblocking(self) {
        self.set_stop();
    }

    fn set_stop(&self) {
        let _lock = self.data.inner.lock().unwrap();
        self.stop.store(true, Ordering::Relaxed);
        self.data.changed.notify_all();
    }

    #[deprecated = "`Debouncer` provides all methods from `Watcher` itself now. Remove `.watcher()` and use those methods directly."]
    pub fn watcher(&mut self) {}

    #[deprecated = "`Debouncer` now manages root paths automatically. Remove all calls to `add_root` and `remove_root`."]
    pub fn cache(&mut self) {}

    // ===== watch roots ===================================================

    /// Register a root path so the debouncer can pick the correct recursive
    /// mode and rescan origin for events below it, and seed the file ID cache.
    fn add_root(&mut self, path: impl Into<PathBuf>, recursive_mode: RecursiveMode) {
        let path = path.into();

        let mut data = self.data.inner.lock().unwrap();

        match data
            .roots
            .binary_search_by(|(p, _)| p.as_path().cmp(path.as_path()))
        {
            Ok(_) => return, // already registered
            Err(pos) => {
                // `VecDeque::insert` is O(min(pos, len - pos))
                data.roots.insert(pos, (path.clone(), recursive_mode));
            }
        }

        let mut file_id_cache = self.file_id_cache.lock().unwrap();

        file_id_cache.add_path(&path, recursive_mode);
    }

    /// Drop a root path (and everything below it) from the root set and the file ID cache.
    fn remove_root(&mut self, path: impl AsRef<Path>) {
        let mut data = self.data.inner.lock().unwrap();

        data.roots.retain(|(root, _)| !root.starts_with(path.as_ref()));

        let mut file_id_cache = self.file_id_cache.lock().unwrap();

        file_id_cache.remove_path(path.as_ref());
    }

    pub fn watch(
        &mut self,
        path: impl AsRef<Path>,
        recursive_mode: RecursiveMode,
    ) -> notify::Result<()> {
        self.watcher.watch(path.as_ref(), recursive_mode)?;
        self.add_root(path.as_ref(), recursive_mode);
        Ok(())
    }

    pub fn unwatch(&mut self, path: impl AsRef<Path>) -> notify::Result<()> {
        self.watcher.unwatch(path.as_ref())?;
        self.remove_root(path);
        Ok(())
    }

    pub fn watched_paths(&self) -> notify::Result<Vec<(PathBuf, RecursiveMode)>> {
        self.watcher.watched_paths()
    }

    /// Add/remove paths to watch in batch.
    ///
    /// For some [`Watcher`] implementations this method provides better performance than multiple
    /// calls to [`Watcher::watch`] and [`Watcher::unwatch`] if you want to add/remove many paths at once.
    ///
    /// # Errors
    ///
    /// Returns [`UpdatePathsError`] if any operation fails. Operations are applied sequentially.
    /// When an error occurs, processing stops: operations before `origin` have been applied,
    /// `origin` is the operation that failed (if known), and `remaining` are the operations that
    /// were not attempted. `remaining` does not include `origin`.
    ///
    /// # Examples
    ///
    /// ```
    /// # use notify::{Watcher, RecursiveMode, PathOp};
    /// # use notify_debouncer_full::{RecommendedCache, new_debouncer_opt};
    /// # use std::path::{Path, PathBuf};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// # let mut debouncer = new_debouncer_opt::<_, notify::NullWatcher, _>(
    /// #     std::time::Duration::from_secs(1),
    /// #     None,
    /// #     |e| {},
    /// #     RecommendedCache::new(),
    /// #     Default::default()
    /// # )?;
    /// debouncer.update_paths([
    ///     PathOp::watch_recursive("path/to/file"),
    ///     PathOp::unwatch("path/to/file2"),
    /// ])?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn update_paths<Op: Into<PathOp>>(
        &mut self,
        ops: impl IntoIterator<Item = Op>,
    ) -> std::result::Result<(), UpdatePathsError> {
        let mut paths = Vec::new();
        let ops: Vec<_> = ops
            .into_iter()
            .map(Into::into)
            .inspect(|op| {
                paths.push((
                    op.as_path().to_path_buf(),
                    match op {
                        PathOp::Watch(_, config) => Some(config.recursive_mode()),
                        PathOp::Unwatch(_) => None,
                    },
                ));
            })
            .collect();

        let res = self.watcher.update_paths(ops);
        let updated_len = match res.as_ref() {
            Ok(()) => paths.len(),
            Err(e) => {
                let failed = usize::from(e.origin.is_some());
                paths.len().saturating_sub(e.remaining.len() + failed)
            }
        };
        let updated_paths = &paths[..updated_len];
        for (path, watch_mode) in updated_paths {
            match watch_mode {
                Some(recursive_mode) => self.add_root(path, *recursive_mode),
                None => self.remove_root(path),
            }
        }
        res
    }

    pub fn configure(&mut self, option: notify::Config) -> notify::Result<bool> {
        self.watcher.configure(option)
    }

    #[must_use]
    pub fn kind() -> WatcherKind
    where
        Self: Sized,
    {
        T::kind()
    }

    // ===== event intake ==================================================

    /// Intake for one raw watcher event.
    ///
    /// `inner` holds the pending queues and roots, `cache` holds the file IDs,
    /// and both are owned by this coordinator; when the watcher thread reports
    /// activity it locks are taken in the order used here and that exact record
    /// pair is handed over.
    pub(crate) fn dispatch_event(inner: &mut DebounceDataInner, cache: &mut C, event: Event) {
        log::trace!("raw event: {event:?}");

        if event.need_rescan() {
            let roots = inner.roots.make_contiguous();
            cache.rescan(roots);
            inner.rescan_event = Some(DebouncedEvent { event, time: Self::now() });
            return;
        }

        let path = match event.paths.first() {
            Some(path) => path,
            None => {
                log::info!("skipping event with no paths: {event:?}");
                return;
            }
        };

        match &event.kind {
            EventKind::Create(_) => {
                let recursive_mode = Self::recursive_mode_for(inner, path);

                cache.add_path(path, recursive_mode);

                Self::push_event(inner, event, Self::now());
            }
            EventKind::Modify(ModifyKind::Name(rename_mode)) => {
                match rename_mode {
                    RenameMode::Any => {
                        if event.paths[0].exists() {
                            Self::handle_rename_to(inner, cache, event);
                        } else {
                            Self::handle_rename_from(inner, cache, event);
                        }
                    }
                    RenameMode::To => {
                        Self::handle_rename_to(inner, cache, event);
                    }
                    RenameMode::From => {
                        Self::handle_rename_from(inner, cache, event);
                    }
                    RenameMode::Both => {
                        // ignore and handle `To` and `From` events instead
                    }
                    RenameMode::Other => {
                        // unused
                    }
                }
            }
            EventKind::Remove(_) => {
                Self::push_remove_event(inner, cache, event, Self::now());
            }
            EventKind::Other => {
                // ignore meta events
            }
            _ => {
                if cache.cached_file_id(path).is_none() {
                    let recursive_mode = Self::recursive_mode_for(inner, path);

                    cache.add_path(path, recursive_mode);
                }

                Self::push_event(inner, event, Self::now());
            }
        }
    }

    /// Record an error to re-send later on.
    pub(crate) fn push_error(inner: &mut DebounceDataInner, error: Error) {
        log::trace!("raw error: {error:?}");
        inner.errors.push(error);
    }

    /// Take and return all currently stored errors.
    pub(crate) fn take_errors(inner: &mut DebounceDataInner) -> Vec<Error> {
        std::mem::take(&mut inner.errors)
    }

    // ===== rename handling ===============================================

    fn handle_rename_from(inner: &mut DebounceDataInner, cache: &mut C, event: Event) {
        let time = Self::now();
        let path = &event.paths[0];

        // store event
        let file_id = cache.cached_file_id(path).map(|id| *id.as_ref());
        inner.rename_event = Some((DebouncedEvent::new(event.clone(), time), file_id));

        cache.remove_path(path);

        Self::push_event(inner, event, time);
    }

    fn handle_rename_to(inner: &mut DebounceDataInner, cache: &mut C, event: Event) {
        let recursive_mode = Self::recursive_mode_for(inner, &event.paths[0]);

        cache.add_path(&event.paths[0], recursive_mode);

        let trackers_match = inner
            .rename_event
            .as_ref()
            .and_then(|(e, _)| e.tracker())
            .and_then(|from_tracker| {
                event
                    .attrs
                    .tracker()
                    .map(|to_tracker| from_tracker == to_tracker)
            })
            .unwrap_or_default();

        let file_ids_match = inner
            .rename_event
            .as_ref()
            .and_then(|(_, id)| id.as_ref())
            .and_then(|from_file_id| {
                cache
                    .cached_file_id(&event.paths[0])
                    .map(|to_file_id| from_file_id == to_file_id.as_ref())
            })
            .unwrap_or_default();

        if trackers_match || file_ids_match {
            // connect rename
            let (mut rename_event, _) =
                inner.rename_event.take().unwrap(); // unwrap is safe because `rename_event` must be set at this point
            let path = rename_event.paths.remove(0);
            let time = rename_event.time;
            Self::push_rename_event(inner, cache, path, event, time);
        } else {
            // move in
            Self::push_event(inner, event, Self::now());
        }

        inner.rename_event = None;
    }

    fn push_rename_event(
        inner: &mut DebounceDataInner,
        cache: &mut C,
        path: PathBuf,
        event: Event,
        time: Instant,
    ) {
        cache.remove_path(&path);

        let mut source_queue = inner.queues.remove(&path).unwrap_or_default();

        // remove rename `from` event
        source_queue.events.pop_back();

        // remove existing rename event
        let (remove_index, original_path, original_time) = source_queue
            .events
            .iter()
            .enumerate()
            .find_map(|(index, e)| {
                if matches!(
                    e.kind,
                    EventKind::Modify(ModifyKind::Name(RenameMode::Both))
                ) {
                    Some((Some(index), e.paths[0].clone(), e.time))
                } else {
                    None
                }
            })
            .unwrap_or((None, path, time));

        if let Some(remove_index) = remove_index {
            source_queue.events.remove(remove_index);
        }

        // split off remove or move out event and add it back to the events map
        if Self::queue_was_removed(&source_queue) {
            let event = source_queue.events.pop_front().unwrap();

            inner.queues.insert(
                event.paths[0].clone(),
                Queue {
                    events: [event].into(),
                },
            );
        }

        // update paths
        for e in &mut source_queue.events {
            e.paths = vec![event.paths[0].clone()];
        }

        // insert rename event at the front, unless the file was just created
        if !Self::queue_was_created(&source_queue) {
            source_queue.events.push_front(DebouncedEvent {
                event: Event {
                    kind: EventKind::Modify(ModifyKind::Name(RenameMode::Both)),
                    paths: vec![original_path, event.paths[0].clone()],
                    attrs: event.attrs,
                },
                time: original_time,
            });
        }

        if let Some(target_queue) = inner.queues.get_mut(&event.paths[0]) {
            if !Self::queue_was_created(target_queue) {
                let mut remove_event = DebouncedEvent {
                    event: Event {
                        kind: EventKind::Remove(RemoveKind::Any),
                        paths: vec![event.paths[0].clone()],
                        attrs: Default::default(),
                    },
                    time: original_time,
                };
                if !Self::queue_was_removed(target_queue) {
                    remove_event.event = remove_event.event.set_info("override");
                }
                source_queue.events.push_front(remove_event);
            }
            *target_queue = source_queue;
        } else {
            inner.queues.insert(event.paths[0].clone(), source_queue);
        }
    }

    fn push_remove_event(
        inner: &mut DebounceDataInner,
        cache: &mut C,
        event: Event,
        time: Instant,
    ) {
        let path = &event.paths[0];

        // remove child queues
        inner.queues.retain(|p, _| !p.starts_with(path) || p == path);

        // remove cached file ids
        cache.remove_path(path);

        match inner.queues.get_mut(path) {
            Some(queue) => {
                queue.events = [DebouncedEvent::new(event, time)].into();
            }
            None => {
                Self::push_event(inner, event, time);
            }
        }
    }

    fn push_event(inner: &mut DebounceDataInner, event: Event, time: Instant) {
        let path = &event.paths[0];

        if let Some(queue) = inner.queues.get_mut(path) {
            // Skip duplicate create events and modifications right after creation.
            // This code relies on backends never emitting a `Modify` event with kind other than `Name` for a rename event.
            if match event.kind {
                EventKind::Modify(
                    ModifyKind::Any
                    | ModifyKind::Data(_)
                    | ModifyKind::Metadata(_)
                    | ModifyKind::Other,
                )
                | EventKind::Create(_) => !Self::queue_was_created(queue),
                _ => true,
            } {
                queue.events.push_back(DebouncedEvent::new(event, time));
            }
        } else {
            inner.queues.insert(
                path.to_path_buf(),
                Queue {
                    events: [DebouncedEvent::new(event, time)].into(),
                },
            );
        }
    }

    // ===== queue policy ==================================================

    /// Whether the queue's head event marks the path as freshly created.
    ///
    /// Freshly-created queues swallow create events and the modifications
    /// that follow them, so this policy decides which events are kept`.
    pub(crate) fn queue_was_created(queue: &Queue) -> bool {
        queue.events.front().is_some_and(|event| {
            matches!(
                event.kind,
                EventKind::Create(_) | EventKind::Modify(ModifyKind::Name(RenameMode::To))
            )
        })
    }

    /// Whether the queue's head event marks the path as gone.
    pub(crate) fn queue_was_removed(queue: &Queue) -> bool {
        queue.events.front().is_some_and(|event| {
            matches!(
                event.kind,
                EventKind::Remove(_) | EventKind::Modify(ModifyKind::Name(RenameMode::From))
            )
        })
    }

    /// Find the recursive mode for `path` among the registered `roots`.
    pub(crate) fn recursive_mode_for(inner: &DebounceDataInner, path: &Path) -> RecursiveMode {
        for ancestor in path.ancestors() {
            if let Ok(index) = inner
                .roots
                .binary_search_by(|(root, _)| root.as_path().cmp(ancestor))
            {
                if inner.roots[index].1 == RecursiveMode::Recursive {
                    return RecursiveMode::Recursive;
                }
            }
        }

        RecursiveMode::NonRecursive
    }

    // ===== expiry ========================================================

    /// Collect the debounced events whose debounce window has elapsed, return them sorted.
    pub(crate) fn expire_events(inner: &mut DebounceDataInner) -> Vec<DebouncedEvent> {
        let now = Self::now();
        let mut events_expired = Vec::with_capacity(inner.queues.len());

        if let Some(event) = inner.rescan_event.take() {
            if now.saturating_duration_since(event.time) >= inner.timeout {
                log::trace!("debounced event: {event:?}");
                events_expired.push(event);
            } else {
                inner.rescan_event = Some(event);
            }
        }

        // Visit each queue in place and remove only the ones that become empty.
        inner
            .queues
            .extract_if(|_, queue| {
                let mut kind_index: HashMap<EventKind, usize> = HashMap::default();
                let mut queue_expired = Vec::new();

                while let Some(event) = queue.events.pop_front() {
                    // remove previous event of the same kind
                    if now.saturating_duration_since(event.time) >= inner.timeout {
                        if let Some(idx) = kind_index.insert(event.kind, queue_expired.len()) {
                            queue_expired[idx] = None;
                        }

                        queue_expired.push(Some(event));
                    } else {
                        if let Some(&idx) = kind_index.get(&event.kind) {
                            queue_expired[idx] = None;
                        }
                        queue.events.push_front(event);
                        break;
                    }
                }

                events_expired.extend(queue_expired.into_iter().flatten());

                queue.events.is_empty()
            })
            .for_each(drop);

        Self::sorted_events(events_expired)
    }

    /// Return the given events order by time and path tail.
    pub(crate) fn sorted_events(events: Vec<DebouncedEvent>) -> Vec<DebouncedEvent> {
        let mut sorted = Vec::with_capacity(events.len());

        // group events by path
        let mut groups = Vec::<(PathBuf, VecDeque<DebouncedEvent>)>::new();
        let mut group_indexes: HashMap<PathBuf, usize> = HashMap::default();
        group_indexes.reserve(events.len());
        groups.reserve(events.len());

        for event in events {
            let path = event.paths.last().cloned().unwrap_or_default();

            if let Some(&index) = group_indexes.get(&path) {
                groups[index].1.push_back(event);
            } else {
                group_indexes.insert(path.clone(), groups.len());
                groups.push((path, [event].into()));
            }
        }

        // Keep path order as the tie-breaker for identical timestamps.
        groups.sort_unstable_by(|(left_path, _), (right_path, _)| left_path.cmp(right_path));

        // push events for different paths in chronological order and keep the order of events with the same path

        let mut min_time_heap = groups
            .iter()
            .enumerate()
            .map(|(index, (_, events))| Reverse((events[0].time, index)))
            .collect::<BinaryHeap<_>>();

        while let Some(Reverse((min_time, index))) = min_time_heap.pop() {
            let events = &mut groups[index].1;

            let mut push_next = false;

            while events.front().is_some_and(|event| event.time <= min_time) {
                // unwrap is safe because `pop_front` mus return some in order to enter the loop
                let event = events.pop_front().unwrap();
                sorted.push(event);
                push_next = true;
            }

            if push_next {
                if let Some(event) = events.front() {
                    min_time_heap.push(Reverse((event.time, index)));
                }
            }
        }

        sorted
    }

    // ===== time source ===================================================

    /// The clock every debouncer timestamp comes from.
    ///
    /// The debouncer used to reach for a free-standing clock module; now that
    /// the coordinator owns the whole pipeline the clock belongs to it as well.
    #[cfg(not(test))]
    pub(crate) fn now() -> Instant {
        Instant::now()
    }

    /// Test builds keep using the mockable clock so state-machine tests can
    /// fast-forward time.
    #[cfg(test)]
    pub(crate) fn now() -> Instant {
        crate::time::now()
    }
}

/// [`Debouncer`]'s file ID bookkeeping for the built-in [`FileIdMap`] cache.
///
/// The coordinator owns the file ID duties as much as the queueing duties:
/// scanning paths, forgetting subtrees, rescanning all roots after a backend
/// hiccup, and answering membership queries. [`FileIdMap`] is therefore only a
/// data container; its `FileIdCache` implementation forwards to the associated
/// functions below. The watcher half is irrelevant to the file ID duties, so
/// it is pinned to notify's inert `NullWatcher`.
#[cfg(not(target_family = "wasm"))]
impl Debouncer<notify::NullWatcher, FileIdMap> {
    /// Look up the cached file ID of a path.
    pub(crate) fn cached_file_id_of<'a>(
        cache: &'a FileIdMap,
        path: &Path,
    ) -> Option<&'a FileId> {
        cache.paths.get(path)
    }

    /// Scan a path (recursively below a root, if requested) and remember the
    /// file IDs of everything found.
    pub(crate) fn scan_file_ids(
        cache: &mut FileIdMap,
        path: &Path,
        recursive_mode: RecursiveMode,
    ) {
        let is_recursive = recursive_mode == RecursiveMode::Recursive;

        for (path, file_id) in WalkDir::new(path)
            .follow_links(true)
            .max_depth(Self::dir_scan_depth(is_recursive))
            .into_iter()
            .filter_map(|entry| {
                let path = entry.ok()?.into_path();
                let file_id = get_file_id(&path).ok()?;
                Some((path, file_id))
            })
        {
            cache.paths.insert(path, file_id);
        }
    }

    /// Forget the file IDs of a path and everything below it.
    pub(crate) fn forget_file_ids(cache: &mut FileIdMap, path: &Path) {
        cache.paths.retain(|p, _| !p.starts_with(path));
    }

    /// Re-scan all `root_paths` after the notification back-end has dropped
    /// events and the debouncer asked for a full rescan.
    pub(crate) fn rescan_file_ids(cache: &mut FileIdMap, root_paths: &[(PathBuf, RecursiveMode)]) {
        for (path, recursive_mode) in root_paths {
            Self::scan_file_ids(cache, path, *recursive_mode);
        }
    }

    fn dir_scan_depth(is_recursive: bool) -> usize {
        if is_recursive { usize::MAX } else { 1 }
    }
}

impl<T: Watcher, C: FileIdCache> Drop for Debouncer<T, C> {
    fn drop(&mut self) {
        self.set_stop();
    }
}

/// Creates a new debounced watcher with custom configuration.
///
/// Timeout is the amount of time after which a debounced event is emitted.
///
/// If `tick_rate` is `None`, notify will select a tick rate that is 1/4 of the provided timeout.
pub fn new_debouncer_opt<F: DebounceEventHandler, T: Watcher, C: FileIdCache + Send + 'static>(
    timeout: Duration,
    tick_rate: Option<Duration>,
    event_handler: F,
    file_id_cache: C,
    config: notify::Config,
) -> Result<Debouncer<T, C>, Error> {
    Debouncer::new_opt(timeout, tick_rate, event_handler, file_id_cache, config)
}

/// Short function to create a new debounced watcher with the recommended debouncer and the built-in file ID cache.
///
/// Timeout is the amount of time after which a debounced event is emitted.
///
/// If `tick_rate` is `None`, notify will select a tick rate that is 1/4 of the provided timeout.
pub fn new_debouncer<F: DebounceEventHandler>(
    timeout: Duration,
    tick_rate: Option<Duration>,
    event_handler: F,
) -> Result<Debouncer<RecommendedWatcher, RecommendedCache>, Error> {
    new_debouncer_opt::<F, RecommendedWatcher, RecommendedCache>(
        timeout,
        tick_rate,
        event_handler,
        RecommendedCache::new(),
        notify::Config::default(),
    )
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::{Path, PathBuf},
        time::Duration,
    };

    use super::*;

    use pretty_assertions::assert_eq;
    use rstest::rstest;
    use tempfile::tempdir;
    use testing::TestCase;
    use time::MockTime;

    /// Anchor for the coordinator's associated state-machine and ordering
    /// functions; the test cases below hand plain state and cache parts to
    /// the debouncer directly, so no guard is ever constructed.
    type TestDebouncer = Debouncer<notify::NullWatcher, testing::TestCache>;

    #[derive(Debug)]
    struct FailingWatcher {
        fail_path: PathBuf,
    }

    impl Watcher for FailingWatcher {
        fn new<F: notify::EventHandler>(
            _event_handler: F,
            _config: notify::Config,
        ) -> notify::Result<Self> {
            Ok(Self {
                fail_path: PathBuf::from("bad"),
            })
        }

        fn watch(&mut self, path: &Path, _recursive_mode: RecursiveMode) -> notify::Result<()> {
            if path == self.fail_path {
                Err(Error::path_not_found())
            } else {
                Ok(())
            }
        }

        fn unwatch(&mut self, path: &Path) -> notify::Result<()> {
            if path == self.fail_path {
                Err(Error::path_not_found())
            } else {
                Ok(())
            }
        }

        fn kind() -> WatcherKind {
            WatcherKind::NullWatcher
        }
    }

    #[derive(Debug, Default)]
    struct TrackingWatcher {
        watched: Vec<(PathBuf, RecursiveMode)>,
    }

    impl Watcher for TrackingWatcher {
        fn new<F: notify::EventHandler>(
            _event_handler: F,
            _config: notify::Config,
        ) -> notify::Result<Self> {
            Ok(Self::default())
        }

        fn watch(&mut self, path: &Path, recursive_mode: RecursiveMode) -> notify::Result<()> {
            self.watched.push((path.to_path_buf(), recursive_mode));
            Ok(())
        }

        fn unwatch(&mut self, path: &Path) -> notify::Result<()> {
            let original_len = self.watched.len();
            self.watched
                .retain(|(watched_path, _)| watched_path != path);

            if self.watched.len() == original_len {
                Err(Error::watch_not_found())
            } else {
                Ok(())
            }
        }

        fn kind() -> WatcherKind {
            WatcherKind::NullWatcher
        }

        fn watched_paths(&self) -> notify::Result<Vec<(PathBuf, RecursiveMode)>> {
            Ok(self.watched.clone())
        }
    }

    #[rstest]
    fn state(
        #[values(
            "add_create_and_remove_event",
            "add_create_event",
            "add_create_event_after_remove_event",
            "add_create_dir_event_twice",
            "add_event_with_no_paths_is_ok",
            "add_modify_any_event_after_create_event",
            "add_modify_content_event_after_create_event",
            "add_rename_from_event",
            "add_rename_from_event_after_create_event",
            "add_rename_from_event_after_modify_event",
            "add_rename_from_event_after_create_and_modify_event",
            "add_rename_from_event_after_rename_from_event",
            "add_rename_to_event",
            "add_rename_to_dir_event",
            "add_rename_from_and_to_event",
            "add_rename_from_and_to_event_after_create",
            "add_rename_from_and_to_event_after_rename",
            "add_rename_from_and_to_event_after_modify_content",
            "add_rename_from_and_to_event_override_created",
            "add_rename_from_and_to_event_override_modified",
            "add_rename_from_and_to_event_override_removed",
            "add_rename_from_and_to_event_with_file_ids",
            "add_rename_from_and_to_event_with_different_file_ids",
            "add_rename_from_and_to_event_with_different_tracker",
            "add_rename_both_event",
            "add_remove_event",
            "add_remove_event_after_create_event",
            "add_remove_event_after_modify_event",
            "add_remove_event_after_create_and_modify_event",
            "add_remove_parent_event_after_remove_child_event",
            "add_errors",
            "debounce_modify_events",
            "emit_continuous_modify_content_events",
            "emit_events_in_chronological_order",
            "emit_events_with_a_prepended_rename_event",
            "emit_close_events_only_once",
            "emit_modify_event_after_close_event",
            "emit_needs_rescan_event",
            "read_file_id_without_create_event",
            "sort_events_chronologically",
            "sort_events_with_reordering"
        )]
        file_name: &str,
    ) {
        let file_content =
            fs::read_to_string(Path::new(&format!("./test_cases/{file_name}.hjson"))).unwrap();
        let mut test_case = deser_hjson::from_str::<TestCase>(&file_content).unwrap();

        let time = time::now();
        MockTime::set_time(time);

        let (mut inner, mut cache) = test_case.state.into_debounce_parts(time);
        inner.roots = VecDeque::from([(PathBuf::from("/"), RecursiveMode::Recursive)]);

        let mut prev_event_time = Duration::default();

        for event in test_case.events {
            let event_time = Duration::from_millis(event.time);
            let event = event.into_debounced_event(time, None);
            MockTime::advance(event_time - prev_event_time);
            prev_event_time = event_time;
            TestDebouncer::dispatch_event(&mut inner, &mut cache, event.event);
        }

        for error in test_case.errors {
            let error = error.into_notify_error();
            TestDebouncer::push_error(&mut inner, error);
        }

        let expected_errors = std::mem::take(&mut test_case.expected.errors);
        let expected_events = std::mem::take(&mut test_case.expected.events);
        let (expected_inner, expected_cache) = test_case.expected.into_debounce_parts(time);
        assert_eq!(
            inner.queues, expected_inner.queues,
            "queues not as expected"
        );
        assert_eq!(
            inner.rename_event, expected_inner.rename_event,
            "rename event not as expected"
        );
        assert_eq!(
            inner.rescan_event, expected_inner.rescan_event,
            "rescan event not as expected"
        );
        assert_eq!(
            cache.paths, expected_cache.paths,
            "cache not as expected"
        );

        assert_eq!(
            inner
                .errors
                .iter()
                .map(|e| format!("{e:?}"))
                .collect::<Vec<_>>(),
            expected_errors
                .iter()
                .map(|e| format!("{:?}", e.clone().into_notify_error()))
                .collect::<Vec<_>>(),
            "errors not as expected"
        );

        let backup_time = time::now();
        let backup_queues = inner.queues.clone();

        for (delay, events) in expected_events {
            MockTime::set_time(backup_time);
            inner.queues = backup_queues.clone();

            match delay.as_str() {
                "none" => {}
                "short" => MockTime::advance(Duration::from_millis(10)),
                "long" => MockTime::advance(Duration::from_millis(100)),
                _ => {
                    if let Ok(ts) = delay.parse::<u64>() {
                        MockTime::set_time(time + Duration::from_millis(ts));
                    }
                }
            }

            let events = events
                .into_iter()
                .map(|event| event.into_debounced_event(time, None))
                .collect::<Vec<_>>();

            assert_eq!(
                TestDebouncer::expire_events(&mut inner),
                events,
                "debounced events after a `{delay}` delay"
            );
        }
    }

    #[test]
    fn recursive_mode_uses_recursive_root_for_overlapping_watches() {
        let inner = DebounceDataInner {
            queues: HashMap::default(),
            roots: VecDeque::from([
                (PathBuf::from("root"), RecursiveMode::NonRecursive),
                (PathBuf::from("root/nested"), RecursiveMode::Recursive),
            ]),
            rename_event: None,
            rescan_event: None,
            errors: Vec::new(),
            timeout: Duration::from_millis(50),
        };

        assert_eq!(
            TestDebouncer::recursive_mode_for(&inner, Path::new("root/nested/child")),
            RecursiveMode::Recursive
        );
        assert_eq!(
            TestDebouncer::recursive_mode_for(&inner, Path::new("root/other")),
            RecursiveMode::NonRecursive
        );
    }

    #[test]
    fn sort_events_ties_by_path() {
        let time = time::now();
        let events = vec![
            DebouncedEvent::new(
                Event::new(EventKind::Any).add_path(PathBuf::from("/watch/b")),
                time,
            ),
            DebouncedEvent::new(
                Event::new(EventKind::Any).add_path(PathBuf::from("/watch/a")),
                time,
            ),
        ];

        let sorted = TestDebouncer::sorted_events(events);
        let paths = sorted
            .into_iter()
            .map(|event| event.paths[0].clone())
            .collect::<Vec<_>>();

        assert_eq!(
            paths,
            vec![PathBuf::from("/watch/a"), PathBuf::from("/watch/b")]
        );
    }

    #[test]
    fn integration() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempdir()?;

        // set up the watcher
        let (tx, rx) = std::sync::mpsc::channel();
        let mut debouncer = new_debouncer(Duration::from_millis(10), None, tx)?;
        debouncer.watch(dir.path(), RecursiveMode::Recursive)?;

        // create a new file
        let file_path = dir.path().join("file.txt");
        fs::write(&file_path, b"Lorem ipsum")?;

        println!("waiting for event at {}", file_path.display());

        // wait for up to 10 seconds for the create event, ignore all other events
        let deadline = Instant::now() + Duration::from_secs(10);
        while deadline > Instant::now() {
            let events = rx
                .recv_timeout(deadline - Instant::now())
                .expect("did not receive expected event")
                .expect("received an error");

            for event in events {
                if event.event.paths == vec![file_path.clone()]
                    || event.event.paths == vec![file_path.canonicalize()?]
                {
                    return Ok(());
                }

                println!("unexpected event: {event:?}");
            }
        }

        panic!("did not receive expected event");
    }

    /// Regression test: on macOS, FSEvents reports file deletions as a burst
    /// of `Create(File)` + `Modify(Data)` + `Remove(File)`.  The debouncer
    /// must not suppress the `Remove` event, even though a prior `Create`
    /// for the same path exists in the queue.
    ///
    /// Without the fix, the coordinator's remove handling would see
    /// `queue_was_created() == true` and cancel the entire queue, swallowing
    /// the removal.
    #[test]
    #[cfg(all(target_os = "macos", feature = "macos_fsevent"))]
    fn remove_event_not_swallowed_after_create() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempdir()?;
        let dir_path = dir.path().canonicalize()?;

        let (tx, rx) = std::sync::mpsc::channel();
        let mut debouncer = new_debouncer(Duration::from_millis(10), None, tx)?;
        debouncer.watch(&dir_path, RecursiveMode::NonRecursive)?;

        // Create a file and wait for the debouncer to deliver the Create event.
        let file_path = dir_path.join("ephemeral.txt");
        fs::write(&file_path, b"will be deleted")?;

        let deadline = Instant::now() + Duration::from_secs(5);
        let mut got_create = false;
        while Instant::now() < deadline {
            match rx.recv_timeout(Duration::from_millis(100)) {
                Ok(Ok(events)) => {
                    if events.iter().any(|e| {
                        matches!(e.event.kind, EventKind::Create(_))
                            && e.event.paths.contains(&file_path)
                    }) {
                        got_create = true;
                        break;
                    }
                }
                Ok(Err(_)) => {}
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
                Err(_) => break,
            }
        }
        assert!(got_create, "expected Create event for ephemeral.txt");

        // Drain any remaining events from the creation.
        std::thread::sleep(Duration::from_millis(200));
        while rx.try_recv().is_ok() {}

        // Delete the file.
        fs::remove_file(&file_path)?;

        // The debouncer MUST deliver an event for this path (Remove, or at
        // minimum any event whose path matches so the consumer can stat it).
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut got_removal = false;
        while Instant::now() < deadline {
            match rx.recv_timeout(Duration::from_millis(100)) {
                Ok(Ok(events)) => {
                    if events.iter().any(|e| e.event.paths.contains(&file_path)) {
                        got_removal = true;
                        break;
                    }
                }
                Ok(Err(_)) => {}
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
                Err(_) => break,
            }
        }
        assert!(
            got_removal,
            "expected Remove (or any) event for deleted ephemeral.txt, got none within 5s"
        );

        Ok(())
    }

    /// Unit-level reproducer for the same bug: feed a Create + Remove
    /// sequence into the coordinator directly, with the queue already flushed
    /// between them (simulating the debounce tick).  The Remove must not be
    /// swallowed.
    #[test]
    #[cfg(target_os = "macos")]
    fn push_remove_after_flushed_create() {
        use crate::NoCache;
        use notify::event::{CreateKind, RemoveKind};
        use std::path::PathBuf;

        type MacTestDebouncer = Debouncer<notify::NullWatcher, NoCache>;

        let mut inner = DebounceDataInner {
            queues: HashMap::default(),
            roots: VecDeque::new(),
            rename_event: None,
            rescan_event: None,
            errors: Vec::new(),
            timeout: Duration::from_millis(50),
        };
        let mut cache = NoCache;

        let time = std::time::Instant::now();
        MockTime::set_time(time);

        let path = PathBuf::from("/tmp/test_file.txt");

        // Simulate: file created → Create event added
        MacTestDebouncer::dispatch_event(
            &mut inner,
            &mut cache,
            Event {
                kind: EventKind::Create(CreateKind::File),
                paths: vec![path.clone()],
                ..Default::default()
            },
        );

        // Simulate: debounce tick flushes the Create
        MockTime::advance(Duration::from_millis(100));
        let flushed = MacTestDebouncer::expire_events(&mut inner);
        assert_eq!(flushed.len(), 1);
        assert!(matches!(flushed[0].event.kind, EventKind::Create(_)));

        // Simulate: FSEvents sends Create + Remove for the deletion
        // (this is what macOS does)
        MacTestDebouncer::dispatch_event(
            &mut inner,
            &mut cache,
            Event {
                kind: EventKind::Create(CreateKind::File),
                paths: vec![path.clone()],
                ..Default::default()
            },
        );
        MacTestDebouncer::dispatch_event(
            &mut inner,
            &mut cache,
            Event {
                kind: EventKind::Remove(RemoveKind::File),
                paths: vec![path.clone()],
                ..Default::default()
            },
        );

        // Flush again — we MUST get the Remove event
        MockTime::advance(Duration::from_millis(100));
        let flushed = MacTestDebouncer::expire_events(&mut inner);
        assert!(
            flushed
                .iter()
                .any(|e| matches!(e.event.kind, EventKind::Remove(_))),
            "expected Remove event after flushed Create, got: {flushed:?}"
        );
    }

    #[cfg(feature = "futures")]
    #[tokio::test]
    async fn futures_unbounded_sender_as_handler() {
        use futures::StreamExt;

        let dir = tempdir().unwrap();

        let (tx, mut rx) = futures::channel::mpsc::unbounded();
        let mut debouncer = new_debouncer(Duration::from_millis(10), None, tx).unwrap();
        debouncer
            .watch(dir.path(), RecursiveMode::Recursive)
            .unwrap();

        let file_path = dir.path().join("file.txt");
        fs::write(&file_path, b"Lorem ipsum").unwrap();

        tokio::time::timeout(Duration::from_secs(10), rx.next())
            .await
            .expect("timeout")
            .expect("No event")
            .expect("error");
    }

    #[cfg(feature = "tokio")]
    #[tokio::test]
    async fn tokio_unbounded_sender_as_handler() {
        let dir = tempdir().unwrap();

        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let mut debouncer = new_debouncer(Duration::from_millis(10), None, tx).unwrap();
        debouncer
            .watch(dir.path(), RecursiveMode::Recursive)
            .unwrap();

        let file_path = dir.path().join("file.txt");
        fs::write(&file_path, b"Lorem ipsum").unwrap();

        tokio::time::timeout(Duration::from_secs(10), rx.recv())
            .await
            .expect("timeout")
            .expect("No event")
            .expect("error");
    }

    #[test]
    fn update_paths() -> Result<(), Box<dyn std::error::Error>> {
        let dir1 = tempdir()?;
        let dir2 = tempdir()?;

        // set up the watcher
        let (tx, rx) = std::sync::mpsc::channel();
        let mut debouncer = new_debouncer(Duration::from_millis(10), None, tx)?;
        debouncer.update_paths([
            PathOp::watch_recursive(dir1.path()),
            PathOp::watch_recursive(dir2.path()),
        ])?;

        // create a new file
        let file_path1 = dir1.path().join("file.txt");
        let file_path2 = dir2.path().join("file.txt");
        fs::write(&file_path1, b"Lorem ipsum1")?;
        fs::write(&file_path2, b"Lorem ipsum1")?;

        println!("waiting for events at {file_path1:?} and {file_path2:?}");

        // wait for up to 10 seconds for the create event, ignore all other events
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut received = (false, false);
        while deadline > Instant::now() {
            let events = rx
                .recv_timeout(deadline - Instant::now())
                .expect("did not receive expected event")
                .expect("received an error");

            for event in events {
                println!("event {event:?}");
                if event.event.paths == vec![file_path1.clone()]
                    || event.event.paths == vec![file_path1.canonicalize()?]
                {
                    received.0 = true;
                }

                if event.event.paths == vec![file_path2.clone()]
                    || event.event.paths == vec![file_path2.canonicalize()?]
                {
                    received.1 = true;
                }

                if received == (true, true) {
                    return Ok(());
                }
            }
        }

        panic!("did not receive expected event");
    }

    #[test]
    fn update_paths_error_does_not_add_failed_root() -> Result<(), Box<dyn std::error::Error>> {
        let mut debouncer = new_debouncer_opt::<_, FailingWatcher, NoCache>(
            Duration::from_millis(20),
            Some(Duration::from_millis(5)),
            |_| {},
            NoCache::new(),
            notify::Config::default(),
        )?;

        let err = debouncer
            .update_paths([
                PathOp::watch_recursive("ok1"),
                PathOp::watch_recursive("bad"),
                PathOp::watch_recursive("ok2"),
            ])
            .unwrap_err();
        assert!(err.origin.is_some());
        assert_eq!(err.remaining.len(), 1);

        let roots = debouncer.data.inner.lock().unwrap().roots.clone();
        assert_eq!(
            roots,
            VecDeque::from([(PathBuf::from("ok1"), RecursiveMode::Recursive)])
        );

        Ok(())
    }

    #[test]
    fn watched_paths_with_watch_update_paths_and_unwatch() -> Result<(), Box<dyn std::error::Error>>
    {
        let mut debouncer = new_debouncer_opt::<_, TrackingWatcher, NoCache>(
            Duration::from_millis(20),
            Some(Duration::from_millis(5)),
            |_| {},
            NoCache::new(),
            notify::Config::default(),
        )?;

        let path1 = PathBuf::from("one");
        let path2 = PathBuf::from("two");
        let path3 = PathBuf::from("three");

        assert!(debouncer.watched_paths()?.is_empty());

        debouncer.watch(&path1, RecursiveMode::Recursive)?;
        assert_eq!(
            debouncer.watched_paths()?,
            vec![(path1.clone(), RecursiveMode::Recursive)]
        );

        debouncer.update_paths([
            PathOp::unwatch(&path1),
            PathOp::watch_non_recursive(path2.clone()),
            PathOp::watch_recursive(path3.clone()),
        ])?;
        assert_eq!(
            debouncer.watched_paths()?,
            vec![
                (path2.clone(), RecursiveMode::NonRecursive),
                (path3.clone(), RecursiveMode::Recursive),
            ]
        );

        debouncer.unwatch(&path2)?;
        assert_eq!(
            debouncer.watched_paths()?,
            vec![(path3, RecursiveMode::Recursive)]
        );

        Ok(())
    }
}
