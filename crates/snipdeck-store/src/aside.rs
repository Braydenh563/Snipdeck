//! Setting a damaged library aside for a rebuild, and undoing that after a crash.
//!
//! The damaged library moves into one folder, `snipdeck.damaged-<stamp>/`, holding the
//! database, its log, its shared-memory file and `blobs/`. A marker file, `snipdeck.rebuild`,
//! is written before the first rename and removed by [`finish_rebuild`] (the commit point).
//! [`Store::open`](crate::Store::open) calls [`recover`] under the folder lock, so a half-moved
//! library is put back before any database is created.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::store::FolderLock;
use crate::{Error, Result};

/// The marker file inside the data folder.
const MARKER: &str = "snipdeck.rebuild";
const MARKER_TMP: &str = "snipdeck.rebuild.tmp";
/// The temporary archive of what a damaged database could still give up.
const SALVAGE: &str = "snipdeck.salvage.snipdeck";
const MARKER_HEAD: &str = "snipdeck-rebuild 1";
const ASIDE_PREFIX: &str = "snipdeck.damaged-";
/// The names that move, in the order they move. Fixed here, never read from the marker.
const NAMES: [&str; 4] = ["snipdeck.db", "snipdeck.db-wal", "snipdeck.db-shm", "blobs"];

/// Renames a file or folder; the seam through which tests inject a failure.
pub(crate) trait Rename {
    fn rename(&mut self, from: &Path, to: &Path) -> io::Result<()>;
}

/// The real thing.
pub(crate) struct RealRename;

impl Rename for RealRename {
    fn rename(&mut self, from: &Path, to: &Path) -> io::Result<()> {
        fs::rename(from, to)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    Moving,
    Fresh,
}

#[derive(Debug, PartialEq, Eq)]
struct Marker {
    aside: String,
    phase: Phase,
}

/// A plain `snipdeck.damaged-*` name: no separators, no `..`.
fn is_aside_name(name: &str) -> bool {
    name.starts_with(ASIDE_PREFIX)
        && name.len() > ASIDE_PREFIX.len()
        && !name.contains(['/', '\\', ':', '\0'])
        && !name.contains("..")
}

fn parse(text: &str) -> Option<Marker> {
    let mut lines = text.lines();
    if lines.next()? != MARKER_HEAD {
        return None;
    }
    let aside = lines.next()?.strip_prefix("aside=")?;
    let phase = match lines.next()?.strip_prefix("phase=")? {
        "moving" => Phase::Moving,
        "fresh" => Phase::Fresh,
        _ => return None,
    };
    if lines.next()? != "end" || lines.next().is_some() || !is_aside_name(aside) {
        return None;
    }
    Some(Marker {
        aside: aside.to_owned(),
        phase,
    })
}

fn render(aside: &str, phase: Phase) -> String {
    let phase = match phase {
        Phase::Moving => "moving",
        Phase::Fresh => "fresh",
    };
    format!("{MARKER_HEAD}\naside={aside}\nphase={phase}\nend\n")
}

/// Writes the marker whole or not at all: temporary file, `sync_all`, rename over.
fn write_marker(folder: &Path, aside: &str, phase: Phase) -> io::Result<()> {
    use io::Write as _;
    let tmp = folder.join(MARKER_TMP);
    let mut file = fs::File::create(&tmp)?;
    file.write_all(render(aside, phase).as_bytes())?;
    file.sync_all()?;
    drop(file);
    fs::rename(&tmp, folder.join(MARKER))
}

fn exists(path: &Path) -> bool {
    path.symlink_metadata().is_ok()
}

/// Moves the library in `folder` (database, log, shared memory, blobs) into the new folder
/// `<folder>/<stem>/`, which must be a plain `snipdeck.damaged-*` name that does not exist.
/// The caller has closed its store ([`Store::close`](crate::Store::close)) and holds `lock`.
///
/// On a failure part-way, what moved is put back and the marker removed; if even that fails
/// the marker stays and the next [`Store::open`](crate::Store::open) retries the undo.
pub fn set_aside(folder: &Path, lock: &FolderLock, stem: &str) -> Result<()> {
    set_aside_with(&mut RealRename, folder, lock, stem)
}

pub(crate) fn set_aside_with(
    r: &mut dyn Rename,
    folder: &Path,
    lock: &FolderLock,
    stem: &str,
) -> Result<()> {
    if !is_aside_name(stem) {
        return Err(Error::Invalid(format!("`{stem}` is not a set-aside name")));
    }
    let aside = folder.join(stem);
    if exists(&aside) || exists(&folder.join(MARKER)) {
        return Err(Error::Invalid(
            "a set-aside folder or rebuild marker already exists".to_owned(),
        ));
    }
    write_marker(folder, stem, Phase::Moving)?;
    let moved = fs::create_dir(&aside).map_err(Error::from).and_then(|()| {
        for name in NAMES {
            let from = folder.join(name);
            if exists(&from) {
                r.rename(&from, &aside.join(name))?;
            }
        }
        Ok(())
    });
    match moved {
        Ok(()) => write_marker(folder, stem, Phase::Fresh).map_err(|e| {
            // The marker still says `moving`, which is also what undo expects to find.
            let _ = recover_with(r, folder, lock);
            e.into()
        }),
        Err(e) => {
            // Best effort; on failure the marker stays for the next start.
            let _ = recover_with(r, folder, lock);
            Err(e)
        }
    }
}

/// Puts a set-aside library back, deleting the partial fresh one if there is one, and removes
/// the marker. Without a marker it does nothing. `Err(RebuildInterrupted)` when it cannot.
pub fn undo_set_aside(folder: &Path, lock: &FolderLock) -> Result<()> {
    recover(folder, lock)
}

/// The rebuild succeeded: removes the marker (the commit point) and the salvage file.
pub fn finish_rebuild(folder: &Path, _lock: &FolderLock) -> Result<()> {
    let _ = fs::remove_file(folder.join(SALVAGE));
    match fs::remove_file(folder.join(MARKER)) {
        Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e.into()),
        _ => Ok(()),
    }
}

/// Undoes an interrupted rebuild under the folder lock, before any connection is opened.
pub(crate) fn recover(folder: &Path, lock: &FolderLock) -> Result<()> {
    recover_with(&mut RealRename, folder, lock)
}

pub(crate) fn recover_with(r: &mut dyn Rename, folder: &Path, _lock: &FolderLock) -> Result<()> {
    let interrupted = |aside: PathBuf| Error::RebuildInterrupted {
        folder: folder.to_owned(),
        aside,
    };
    let marker_path = folder.join(MARKER);
    let text = match fs::read(&marker_path) {
        Ok(bytes) => String::from_utf8(bytes).ok(),
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(_) => None,
    };
    let Some(marker) = text.as_deref().and_then(parse) else {
        return Err(interrupted(folder.join(format!("{ASIDE_PREFIX}*"))));
    };
    let aside = folder.join(&marker.aside);
    let stuck = || interrupted(aside.clone());

    // Check everything before changing anything.
    match marker.phase {
        Phase::Moving => {
            if NAMES
                .iter()
                .any(|n| exists(&aside.join(n)) && exists(&folder.join(n)))
            {
                return Err(stuck());
            }
        }
        Phase::Fresh => {
            if !aside.join(NAMES[0]).is_file() {
                return Err(stuck());
            }
            // Only data from the backup and salvage archives: deleting it loses nothing.
            for name in NAMES {
                let path = folder.join(name);
                let removed = match path.symlink_metadata() {
                    Err(_) => Ok(()),
                    Ok(m) if m.is_dir() => fs::remove_dir_all(&path),
                    Ok(_) => fs::remove_file(&path),
                };
                if removed.is_err() {
                    return Err(stuck());
                }
            }
        }
    }
    for name in NAMES {
        let from = aside.join(name);
        if exists(&from) && r.rename(&from, &folder.join(name)).is_err() {
            return Err(stuck());
        }
    }
    if exists(&aside) && fs::remove_dir(&aside).is_err() {
        return Err(stuck());
    }
    let _ = fs::remove_file(folder.join(SALVAGE));
    fs::remove_file(&marker_path).map_err(|_| stuck())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::time::Duration;

    use snipdeck_core::{Format, Formats, INLINE_LIMIT, Item, ItemKind, Placement};

    use super::*;
    use crate::{
        BlobStore as _, BoardRepo as _, ItemRepo as _, ListRepo as _, Store, TrashRepo as _,
        Trashable,
    };

    const STAMP: &str = "snipdeck.damaged-20261107-1030";

    /// Fails the rename that is call number `.0` (counting from 0), and only that one.
    struct FailAt(usize, usize);

    impl Rename for FailAt {
        fn rename(&mut self, from: &Path, to: &Path) -> io::Result<()> {
            self.1 += 1;
            if self.1 - 1 == self.0 {
                return Err(io::Error::other("injected"));
            }
            fs::rename(from, to)
        }
    }

    fn ts() -> snipdeck_core::Timestamp {
        snipdeck_core::Timestamp::from_second(1_800_000_000).unwrap()
    }

    /// Adds an item (with a blob when `blob`) and returns its title.
    fn add(store: &mut Store, title: &str, blob: Option<Vec<u8>>) {
        use snipdeck_core::{Board, List, Name};
        let boards = store.boards().unwrap();
        let board = if let Some(b) = boards.first() {
            b.clone()
        } else {
            let b = Board::new(Name::new("b").unwrap(), ts());
            store.insert_board(&b, Placement::Last).unwrap();
            b
        };
        let lists = store.lists(board.id).unwrap();
        let list = if let Some(l) = lists.first() {
            l.clone()
        } else {
            let l = List::new(board.id, Name::new("l").unwrap(), ts());
            store.insert_list(&l, Placement::Last).unwrap();
            l
        };
        let content = match blob {
            Some(bytes) => store.put(bytes).unwrap(),
            None => snipdeck_core::Content::Inline(
                snipdeck_core::Inline::new(title.as_bytes().to_vec()).unwrap(),
            ),
        };
        let format = if matches!(content, snipdeck_core::Content::Blob { .. }) {
            Format::Png(content)
        } else {
            Format::Text(content)
        };
        let kind = if matches!(format, Format::Png(_)) {
            ItemKind::Image
        } else {
            ItemKind::Text
        };
        let item = Item::new(list.id, title.to_owned(), kind, ts());
        store
            .insert_item(&item, &Formats::new(vec![format]).unwrap(), Placement::Last)
            .unwrap();
    }

    fn titles(store: &Store) -> Vec<String> {
        let mut t: Vec<String> = store
            .all_items()
            .unwrap()
            .into_iter()
            .map(|i| i.title)
            .collect();
        t.sort();
        t
    }

    /// A data folder holding a closed library: two items, one with a blob.
    fn library(dir: &Path) -> FolderLock {
        let mut s = Store::open(dir).unwrap();
        add(&mut s, "one", None);
        add(&mut s, "two", Some(vec![7; INLINE_LIMIT + 1]));
        s.close().lock.unwrap()
    }

    /// Every file under `dir`, relative, with its size.
    fn listing(dir: &Path) -> BTreeMap<String, u64> {
        fn walk(base: &Path, dir: &Path, out: &mut BTreeMap<String, u64>) {
            for e in fs::read_dir(dir).unwrap().flatten() {
                let meta = e.metadata().unwrap();
                let rel = e.path().strip_prefix(base).unwrap().display().to_string();
                if meta.is_dir() {
                    out.insert(format!("{rel}/"), 0);
                    walk(base, &e.path(), out);
                } else {
                    out.insert(rel, meta.len());
                }
            }
        }
        let mut out = BTreeMap::new();
        walk(dir, dir, &mut out);
        out
    }

    fn marker(dir: &Path, aside: &str, phase: &str) {
        fs::write(
            dir.join(MARKER),
            format!("{MARKER_HEAD}\naside={aside}\nphase={phase}\nend\n"),
        )
        .unwrap();
    }

    #[test]
    fn closing_checkpoints_so_the_database_file_alone_holds_every_commit() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = Store::open(dir.path()).unwrap();
        add(&mut s, "one", None);
        add(&mut s, "two", None);
        let closed = s.close();
        assert!(closed.self_contained);
        let wal = dir.path().join("snipdeck.db-wal");
        assert!(!wal.exists() || fs::metadata(&wal).unwrap().len() == 0);
        let copy = tempfile::tempdir().unwrap();
        fs::copy(
            dir.path().join("snipdeck.db"),
            copy.path().join("snipdeck.db"),
        )
        .unwrap();
        let copied = Store::open(copy.path()).unwrap();
        assert_eq!(titles(&copied), ["one", "two"]);
    }

    #[test]
    fn set_aside_moves_the_database_its_log_and_the_blobs_into_one_folder() {
        let dir = tempfile::tempdir().unwrap();
        let lock = library(dir.path());
        set_aside(dir.path(), &lock, STAMP).unwrap();
        for n in NAMES {
            assert!(!dir.path().join(n).exists(), "{n} still at the top");
        }
        let text = fs::read_to_string(dir.path().join(MARKER)).unwrap();
        assert_eq!(
            parse(&text),
            Some(Marker {
                aside: STAMP.to_owned(),
                phase: Phase::Fresh
            })
        );
        drop(lock);
        let aside = Store::open(dir.path().join(STAMP)).unwrap();
        assert_eq!(aside.check_integrity().unwrap(), Vec::<String>::new());
        assert_eq!(titles(&aside), ["one", "two"]);
    }

    #[test]
    fn a_fresh_library_sweeping_and_deleting_leaves_the_set_aside_blobs_alone() {
        let dir = tempfile::tempdir().unwrap();
        let lock = library(dir.path());
        set_aside(dir.path(), &lock, STAMP).unwrap();
        let mut fresh = Store::open_locked(dir.path(), lock).unwrap();
        add(&mut fresh, "shared", Some(vec![7; INLINE_LIMIT + 1]));
        let id = fresh.all_items().unwrap()[0].id;
        fresh.trash(Trashable::Item(id), ts()).unwrap();
        fresh.purge_entry(Trashable::Item(id)).unwrap();
        fresh.gc_blobs(Duration::ZERO).unwrap();
        let aside = Store::open(dir.path().join(STAMP)).unwrap();
        assert_eq!(aside.check_integrity().unwrap(), Vec::<String>::new());
    }

    #[test]
    fn a_rename_that_fails_midway_puts_back_what_moved() {
        for n in 0..4 {
            let dir = tempfile::tempdir().unwrap();
            let lock = library(dir.path());
            fs::create_dir_all(dir.path().join("blobs")).unwrap();
            // The shm file may or may not exist after close; make all four names present.
            for extra in ["snipdeck.db-wal", "snipdeck.db-shm"] {
                let p = dir.path().join(extra);
                if !p.exists() {
                    fs::write(p, b"").unwrap();
                }
            }
            let before = listing(dir.path());
            let err = set_aside_with(&mut FailAt(n, 0), dir.path(), &lock, STAMP);
            assert!(err.is_err(), "n={n}");
            assert_eq!(listing(dir.path()), before, "n={n}");
            drop(lock);
            assert_eq!(titles(&Store::open(dir.path()).unwrap()), ["one", "two"]);
        }
    }

    #[test]
    fn a_crash_while_moving_is_undone_at_the_next_open() {
        for k in 0..=4 {
            let dir = tempfile::tempdir().unwrap();
            drop(library(dir.path()));
            for extra in ["snipdeck.db-wal", "snipdeck.db-shm"] {
                let p = dir.path().join(extra);
                if !p.exists() {
                    fs::write(p, b"").unwrap();
                }
            }
            marker(dir.path(), STAMP, "moving");
            let aside = dir.path().join(STAMP);
            fs::create_dir(&aside).unwrap();
            for name in &NAMES[..k] {
                fs::rename(dir.path().join(name), aside.join(name)).unwrap();
            }
            let s = Store::open(dir.path()).unwrap();
            assert_eq!(titles(&s), ["one", "two"], "k={k}");
            assert!(!dir.path().join(MARKER).exists(), "k={k}");
            assert!(!aside.exists(), "k={k}");
        }
    }

    #[test]
    fn a_crash_after_moving_drops_the_partial_fresh_library_and_restores_the_original() {
        let dir = tempfile::tempdir().unwrap();
        let lock = library(dir.path());
        set_aside(dir.path(), &lock, STAMP).unwrap();
        let mut fresh = Store::open_locked(dir.path(), lock).unwrap();
        add(&mut fresh, "half imported", Some(vec![9; INLINE_LIMIT + 1]));
        // A crash: the fresh store is gone without `finish_rebuild`.
        drop(fresh);
        fs::write(dir.path().join(SALVAGE), b"x").unwrap();
        let s = Store::open(dir.path()).unwrap();
        assert_eq!(titles(&s), ["one", "two"]);
        assert_eq!(s.check_integrity().unwrap(), Vec::<String>::new());
        assert!(!dir.path().join(MARKER).exists());
        assert!(!dir.path().join(STAMP).exists());
        assert!(!dir.path().join(SALVAGE).exists());
    }

    #[test]
    fn an_impossible_recovery_changes_nothing_and_refuses_to_open() {
        // (name, setup)
        type Setup = fn(&Path);
        let cases: [(&str, Setup); 3] = [
            ("unreadable marker", |d| {
                fs::write(
                    d.join(MARKER),
                    "snipdeck-rebuild 1\naside=snipdeck.damaged-x\n",
                )
                .unwrap();
            }),
            ("name at both places", |d| {
                marker(d, STAMP, "moving");
                fs::create_dir(d.join(STAMP)).unwrap();
                fs::write(d.join(STAMP).join("snipdeck.db"), b"aside").unwrap();
            }),
            ("missing aside db", |d| {
                marker(d, STAMP, "fresh");
                fs::create_dir(d.join(STAMP)).unwrap();
            }),
        ];
        for (what, setup) in cases {
            let dir = tempfile::tempdir().unwrap();
            drop(library(dir.path()));
            setup(dir.path());
            let before = listing(dir.path());
            let err = Store::open(dir.path()).unwrap_err();
            assert!(
                matches!(err, Error::RebuildInterrupted { .. }),
                "{what}: {err}"
            );
            assert_eq!(listing(dir.path()), before, "{what}");
        }
        // Nothing was created where there was no database.
        let dir = tempfile::tempdir().unwrap();
        marker(dir.path(), STAMP, "fresh");
        assert!(Store::open(dir.path()).is_err());
        assert!(!dir.path().join("snipdeck.db").exists());
    }

    #[test]
    fn an_aside_name_with_a_path_is_an_unreadable_marker() {
        for aside in [
            "../x",
            "a/b",
            "snipdeck.damaged-../x",
            "snipdeck.damaged-a/b",
            "other",
        ] {
            let text = format!("{MARKER_HEAD}\naside={aside}\nphase=moving\nend\n");
            assert_eq!(parse(&text), None, "{aside}");
        }
        let dir = tempfile::tempdir().unwrap();
        let lock = FolderLock::acquire(dir.path()).unwrap();
        assert!(set_aside(dir.path(), &lock, "../x").is_err());
    }

    #[test]
    fn a_held_folder_lock_keeps_a_second_open_out_during_the_swap() {
        let dir = tempfile::tempdir().unwrap();
        let lock = library(dir.path());
        set_aside(dir.path(), &lock, STAMP).unwrap();
        let before = fs::read(dir.path().join(MARKER)).unwrap();
        assert!(matches!(Store::open(dir.path()), Err(Error::Locked { .. })));
        assert_eq!(fs::read(dir.path().join(MARKER)).unwrap(), before);
        drop(lock);
    }
}
