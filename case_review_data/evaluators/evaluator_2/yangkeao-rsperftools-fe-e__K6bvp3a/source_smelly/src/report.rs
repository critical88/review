// Copyright 2019 TiKV Project Authors. Licensed under Apache-2.0.

use std::collections::HashMap;
use std::fmt::{Debug, Formatter};

use spin::RwLock;

use crate::backtrace::Frame;
use crate::frames::resolve_in_perfmap;
use crate::frames::{Frames, Symbol, UnresolvedFrames};
use crate::profiler::Profiler;
use crate::timer::ReportTiming;

use crate::{Error, Result};

/// The final presentation of a report which is actually an `HashMap` from `Frames` to isize (count).
pub struct Report {
    /// Key is a backtrace captured by profiler and value is count of it.
    pub data: HashMap<Frames, isize>,

    /// Collection frequency, start time, duration.
    pub timing: ReportTiming,
}

/// The presentation of an unsymbolicated report which is actually an `HashMap` from `UnresolvedFrames` to isize (count).
pub struct UnresolvedReport {
    /// key is a backtrace captured by profiler and value is count of it.
    pub data: HashMap<UnresolvedFrames, isize>,

    /// Collection frequency, start time, duration.
    pub timing: ReportTiming,
}

type FramesPostProcessor = Box<dyn Fn(&mut Frames)>;

/// Resolve one collected sample into its report presentation.
///
/// Every sample is stored by the profiler in its unresolved form; the symbol
/// names, source files and line numbers are attached here, while the report is
/// being built.
fn resolved_frames(item: UnresolvedFrames) -> Frames {
    let mut fs = Vec::new();

    let mut frame_iter = item.frames.iter();

    while let Some(frame) = frame_iter.next() {
        let mut symbols: Vec<Symbol> = Vec::new();

        if let Some(perfmap_symbol) = resolve_in_perfmap(frame.ip() as usize) {
            symbols.push(perfmap_symbol);
        } else {
            frame.resolve_symbol(|symbol| {
                let symbol = Symbol::from(symbol);
                symbols.push(symbol);
            });
        }

        if symbols.iter().any(|symbol| {
            // macOS prepends an underscore even with `#[no_mangle]`
            matches!(
                &*symbol.name(),
                "perf_signal_handler" | "_perf_signal_handler"
            )
        }) {
            // ignore frame itself and its next one
            frame_iter.next();
            continue;
        }

        if !symbols.is_empty() {
            fs.push(symbols);
        }
    }

    Frames {
        frames: fs,
        thread_name: String::from_utf8_lossy(&item.thread_name[0..item.thread_name_length])
            .into_owned(),
        thread_id: item.thread_id,
        sample_timestamp: item.sample_timestamp,
    }
}

/// A builder of `Report` and `UnresolvedReport`. It builds report from a running `Profiler`.
pub struct ReportBuilder<'a> {
    frames_post_processor: Option<FramesPostProcessor>,
    profiler: &'a RwLock<Result<Profiler>>,
    timing: ReportTiming,
}

impl<'a> ReportBuilder<'a> {
    pub(crate) fn new(profiler: &'a RwLock<Result<Profiler>>, timing: ReportTiming) -> Self {
        Self {
            frames_post_processor: None,
            profiler,
            timing,
        }
    }

    /// Set `frames_post_processor` of a `ReportBuilder`. Before finally building a report, `frames_post_processor`
    /// will be applied to every Frames.
    pub fn frames_post_processor<T>(&mut self, frames_post_processor: T) -> &mut Self
    where
        T: Fn(&mut Frames) + 'static,
    {
        self.frames_post_processor
            .replace(Box::new(frames_post_processor));

        self
    }

    /// Build an `UnresolvedReport`
    pub fn build_unresolved(&self) -> Result<UnresolvedReport> {
        let mut hash_map = HashMap::new();

        match self.profiler.read().as_ref() {
            Err(err) => {
                log::error!("Error in creating profiler: {}", err);
                Err(Error::CreatingError)
            }
            Ok(profiler) => {
                profiler.data.try_iter()?.for_each(|entry| {
                    let count = entry.count;
                    if count > 0 {
                        let key = &entry.item;
                        match hash_map.get_mut(key) {
                            Some(value) => {
                                *value += count;
                            }
                            None => {
                                match hash_map.insert(key.clone(), count) {
                                    None => {}
                                    Some(_) => {
                                        unreachable!();
                                    }
                                };
                            }
                        }
                    }
                });

                Ok(UnresolvedReport {
                    data: hash_map,
                    timing: self.timing.clone(),
                })
            }
        }
    }

    /// Build a `Report`.
    pub fn build(&self) -> Result<Report> {
        let mut hash_map = HashMap::new();

        match self.profiler.write().as_mut() {
            Err(err) => {
                log::error!("Error in creating profiler: {}", err);
                Err(Error::CreatingError)
            }
            Ok(profiler) => {
                profiler.data.try_iter()?.for_each(|entry| {
                    let count = entry.count;
                    if count > 0 {
                        let mut key = resolved_frames(entry.item.clone());
                        if let Some(processor) = &self.frames_post_processor {
                            processor(&mut key);
                        }

                        match hash_map.get_mut(&key) {
                            Some(value) => {
                                *value += count;
                            }
                            None => {
                                match hash_map.insert(key, count) {
                                    None => {}
                                    Some(_) => {
                                        unreachable!();
                                    }
                                };
                            }
                        }
                    }
                });

                Ok(Report {
                    data: hash_map,
                    timing: self.timing.clone(),
                })
            }
        }
    }
}

/// This will generate Report in a human-readable format:
///
/// ```shell
/// FRAME: pprof::profiler::perf_signal_handler::h7b995c4ab2e66493 -> FRAME: Unknown -> FRAME: {func1} ->
/// FRAME: {func2} -> FRAME: {func3} ->  THREAD: {thread_name} {count}
/// ```
impl Debug for Report {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        for (key, val) in self.data.iter() {
            write!(f, "{:?} {}", key, val)?;
            writeln!(f)?;
        }

        Ok(())
    }
}

/// This will generate one sample in the human-readable format of a `Report`'s frames:
///
/// ```shell
/// FRAME: {func1} -> FRAME: {func2} -> FRAME: {func3} -> THREAD: {thread_name} {count}
/// ```
impl Debug for Frames {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        for frame in self.frames.iter() {
            write!(f, "FRAME: ")?;
            for symbol in frame.iter() {
                write!(f, "{} -> ", symbol)?;
            }
        }
        write!(f, "THREAD: ")?;
        if !self.thread_name.is_empty() {
            write!(f, "{}", self.thread_name)
        } else {
            write!(f, "ThreadId({})", self.thread_id)
        }
    }
}

#[cfg(feature = "flamegraph")]
mod flamegraph {
    use super::*;
    use inferno::flamegraph;
    use std::fmt::Write;

    /// Render one sample as a single collapsed-stack line for the flamegraph.
    fn stack_line(stack_key: &Frames, hits: &isize) -> String {
        let mut line = String::new();
        if !stack_key.thread_name.is_empty() {
            line.push_str(&stack_key.thread_name);
        } else {
            write!(line, "{:?}", stack_key.thread_id).unwrap();
        }
        line.push(';');

        for frame in stack_key.frames.iter().rev() {
            for symbol in frame.iter().rev() {
                write!(&mut line, "{};", symbol).unwrap();
            }
        }

        line.pop().unwrap_or_default();
        write!(&mut line, " {}", hits).unwrap();

        line
    }

    impl Report {
        /// `flamegraph` will write an svg flamegraph into `writer` **only available with `flamegraph` feature**
        pub fn flamegraph<W>(&self, writer: W) -> Result<()>
        where
            W: std::io::Write,
        {
            self.flamegraph_with_options(writer, &mut flamegraph::Options::default())
        }

        /// same as `flamegraph`, but accepts custom `options` for the flamegraph
        pub fn flamegraph_with_options<W>(
            &self,
            writer: W,
            options: &mut flamegraph::Options,
        ) -> Result<()>
        where
            W: std::io::Write,
        {
            let lines: Vec<String> = self
                .data
                .iter()
                .map(|(key, value)| stack_line(key, value))
                .collect();
            if !lines.is_empty() {
                flamegraph::from_lines(options, lines.iter().map(|s| &**s), writer).unwrap();
                // TODO: handle this error
            }

            Ok(())
        }
    }
}

#[cfg(feature = "_protobuf")]
#[allow(clippy::useless_conversion)]
#[allow(clippy::needless_update)]
mod protobuf {
    use super::*;
    use crate::protos;
    use std::borrow::Cow;
    use std::collections::HashSet;
    use std::time::SystemTime;

    use symbolic_demangle::demangle;

    const SAMPLES: &str = "samples";
    const COUNT: &str = "count";
    const CPU: &str = "cpu";
    const NANOSECONDS: &str = "nanoseconds";
    const THREAD: &str = "thread";

    /// Returns a thread identifier (name or ID) as a string.
    fn thread_name_or_id(key: &Frames) -> String {
        if !key.thread_name.is_empty() {
            key.thread_name.clone()
        } else {
            format!("{:?}", key.thread_id)
        }
    }

    impl Report {
        /// `pprof` will generate google's pprof format report.
        pub fn pprof(&self) -> crate::Result<protos::Profile> {
            let mut dedup_str = HashSet::new();
            for key in self.data.keys() {
                dedup_str.insert(thread_name_or_id(key));
                for frame in key.frames.iter() {
                    for symbol in frame {
                        dedup_str.insert(
                            demangle(&String::from_utf8_lossy(
                                symbol.name.as_deref().unwrap_or(b"Unknown"),
                            ))
                            .into_owned(),
                        );
                        dedup_str.insert(
                            String::from_utf8_lossy(
                                symbol.name.as_deref().unwrap_or(b"Unknown"),
                            )
                            .into_owned(),
                        );
                        dedup_str.insert(match &symbol.filename {
                            Some(name) => name.as_os_str().to_string_lossy().into_owned(),
                            None => "Unknown".to_string(),
                        });
                    }
                }
            }
            dedup_str.insert(SAMPLES.into());
            dedup_str.insert(COUNT.into());
            dedup_str.insert(CPU.into());
            dedup_str.insert(NANOSECONDS.into());
            dedup_str.insert(THREAD.into());
            // string table's first element must be an empty string
            let mut str_tbl = vec!["".to_owned()];
            str_tbl.extend(dedup_str.into_iter());

            let mut strings = HashMap::new();
            for (index, name) in str_tbl.iter().enumerate() {
                strings.insert(name.as_str(), index);
            }

            let mut samples = vec![];
            let mut loc_tbl = vec![];
            let mut fn_tbl = vec![];
            let mut functions = HashMap::new();
            for (key, count) in self.data.iter() {
                let mut locs = vec![];
                for frame in key.frames.iter() {
                    for symbol in frame {
                        let name = demangle(&String::from_utf8_lossy(
                            symbol.name.as_deref().unwrap_or(b"Unknown"),
                        ))
                        .into_owned();
                        if let Some(loc_idx) = functions.get(&name) {
                            locs.push(*loc_idx);
                            continue;
                        }
                        let sys_name =
                            String::from_utf8_lossy(symbol.name.as_deref().unwrap_or(b"Unknown"));
                        let filename = match &symbol.filename {
                            Some(name) => name.as_os_str().to_string_lossy(),
                            None => Cow::Borrowed("Unknown"),
                        };
                        let lineno = match symbol.lineno {
                            Some(lineno) => lineno,
                            None => 0,
                        };
                        let function_id = fn_tbl.len() as u64 + 1;
                        let function = protos::Function {
                            id: function_id,
                            name: *strings.get(name.as_str()).unwrap() as i64,
                            system_name: *strings.get(sys_name.as_ref()).unwrap() as i64,
                            filename: *strings.get(filename.as_ref()).unwrap() as i64,
                            ..protos::Function::default()
                        };
                        functions.insert(name, function_id);
                        let line = protos::Line {
                            function_id,
                            line: lineno as i64,
                            ..protos::Line::default()
                        };
                        let loc = protos::Location {
                            id: function_id,
                            line: vec![line].into(),
                            ..protos::Location::default()
                        };
                        // the fn_tbl has the same length with loc_tbl
                        fn_tbl.push(function);
                        loc_tbl.push(loc);
                        // current frame locations
                        locs.push(function_id);
                    }
                }
                let thread_name = protos::Label {
                    key: *strings.get(THREAD).unwrap() as i64,
                    str: *strings.get(&thread_name_or_id(key).as_str()).unwrap() as i64,
                    ..protos::Label::default()
                };
                let sample = protos::Sample {
                    location_id: locs,
                    value: vec![
                        *count as i64,
                        *count as i64 * 1_000_000_000 / self.timing.frequency as i64,
                    ],
                    label: vec![thread_name].into(),
                    ..Default::default()
                };
                samples.push(sample);
            }
            let samples_value = protos::ValueType {
                ty: *strings.get(SAMPLES).unwrap() as i64,
                unit: *strings.get(COUNT).unwrap() as i64,
                ..Default::default()
            };
            let time_value = protos::ValueType {
                ty: *strings.get(CPU).unwrap() as i64,
                unit: *strings.get(NANOSECONDS).unwrap() as i64,
                ..Default::default()
            };
            let profile = protos::Profile {
                sample_type: vec![samples_value, time_value.clone()].into(),
                sample: samples.into(),
                string_table: str_tbl.into(),
                function: fn_tbl.into(),
                location: loc_tbl.into(),
                time_nanos: self
                    .timing
                    .start_time
                    .duration_since(SystemTime::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_nanos() as i64,
                duration_nanos: self.timing.duration.as_nanos() as i64,
                period_type: Some(time_value).into(),
                period: 1_000_000_000 / self.timing.frequency as i64,
                ..protos::Profile::default()
            };
            Ok(profile)
        }
    }
}
