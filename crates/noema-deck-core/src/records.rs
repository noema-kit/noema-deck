//! The files the agents keep their sessions in, read where they lie.
//!
//! There are thousands, some of megabytes and some compressed, so each is
//! read once and again only when it changes, on every core, and a line is
//! searched before it is parsed.

use std::{
    collections::HashMap,
    fs,
    io::{BufRead, BufReader, Read},
    num::NonZero,
    panic,
    path::{Path, PathBuf},
    sync::atomic::{AtomicUsize, Ordering},
    thread,
    time::SystemTime,
};

use anyhow::Result;

use crate::agent::{Agent, entries};

/// A file's modification time and length. A change to either means it has
/// to be read again.
type Stamp = (SystemTime, u64);

/// What each file said when last read.
pub(crate) struct Kept<T> {
    read: HashMap<PathBuf, (Stamp, T)>,
}

impl<T> Default for Kept<T> {
    fn default() -> Self {
        Self { read: HashMap::new() }
    }
}

impl<T: Send> Kept<T> {
    /// Reads those of `files` that changed since last time, and forgets the
    /// files no longer among them.
    pub(crate) fn refresh(
        &mut self,
        files: &[(Agent, PathBuf)],
        read: impl Fn(Agent, &Path) -> T + Sync,
    ) -> Result<()> {
        let mut found = HashMap::new();
        let mut unread = Vec::new();
        for (agent, path) in files {
            let Ok(metadata) = fs::metadata(path) else { continue };
            let stamp = (metadata.modified()?, metadata.len());
            match self.read.remove(path).filter(|(read, _)| *read == stamp) {
                Some(known) => _ = found.insert(path.clone(), known),
                None => unread.push((*agent, path.as_path(), stamp)),
            }
        }
        let fresh = in_parallel(&unread, |(agent, path, _)| read(*agent, path));
        for ((_, path, stamp), read) in unread.iter().zip(fresh) {
            found.insert(path.to_path_buf(), (*stamp, read));
        }
        self.read = found;
        Ok(())
    }

    /// What `path` said when last read.
    pub(crate) fn get(&self, path: &Path) -> Option<&T> {
        self.read.get(path).map(|(_, read)| read)
    }
}

/// What `work` makes of each of `items`, in their order, on as many threads
/// as there are cores. Codex compresses old rollouts, and reading every one
/// takes seconds on one thread.
fn in_parallel<I: Sync, T: Send>(items: &[I], work: impl Fn(&I) -> T + Sync) -> Vec<T> {
    let next = AtomicUsize::new(0);
    let workers = thread::available_parallelism().map_or(1, NonZero::get).min(items.len());
    let mut done: Vec<(usize, T)> = thread::scope(|scope| {
        let workers: Vec<_> = (0..workers)
            .map(|_| {
                scope.spawn(|| {
                    let mut done = Vec::new();
                    loop {
                        let at = next.fetch_add(1, Ordering::Relaxed);
                        let Some(item) = items.get(at) else { break };
                        done.push((at, work(item)));
                    }
                    done
                })
            })
            .collect();
        workers
            .into_iter()
            .flat_map(|worker| worker.join().unwrap_or_else(|panic| panic::resume_unwind(panic)))
            .collect()
    });
    done.sort_by_key(|(at, _)| *at);
    done.into_iter().map(|(_, made)| made).collect()
}

/// Calls `each` with every line of `source` whose first `head` bytes hold one
/// of `marks`. Records run to megabytes and most hold nothing wanted, so a
/// line is searched before it is parsed.
pub(crate) fn lines(source: impl Read, head: usize, marks: &[&str], mut each: impl FnMut(&str)) {
    let mut reader = BufReader::with_capacity(1 << 16, source);
    let mut line = Vec::new();
    loop {
        line.clear();
        match reader.read_until(b'\n', &mut line) {
            Ok(0) | Err(_) => break,
            Ok(_) => {}
        }
        let head = &line[..line.len().min(head)];
        if !marks.iter().any(|mark| contains(head, mark.as_bytes())) {
            continue;
        }
        if let Ok(text) = std::str::from_utf8(&line) {
            each(text);
        }
    }
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|window| window == needle)
}

/// Every file under `dir` whose name ends with `suffix`, at most `depth`
/// folders down.
pub(crate) fn walk(dir: &Path, suffix: &str, depth: usize, found: &mut Vec<PathBuf>) -> Result<()> {
    for path in entries(dir)? {
        if path.is_dir() {
            if depth > 0 {
                walk(&path, suffix, depth - 1, found)?;
            }
        } else if path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.ends_with(suffix))
        {
            found.push(path);
        }
    }
    Ok(())
}
