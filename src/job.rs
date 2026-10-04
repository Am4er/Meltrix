use std::cmp::Reverse;
use std::collections::HashSet;
use std::fs;
use std::panic::{self, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU32, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

use eframe::egui;

use crate::Res;

pub const WAIT: u8 = 0;
pub const WORK: u8 = 1;
pub const DONE: u8 = 2;
pub const FAIL: u8 = 3;
pub const STOP: u8 = 4;

pub type Task = Arc<dyn Fn(&Path, &Path, &AtomicBool, &AtomicU32) -> Res<()> + Send + Sync>;

pub struct Job {
    pub src: PathBuf,
    pub name: String,
    pub from: String,
    pub size: u64,
    pub state: AtomicU8,
    pub prog: AtomicU32,
    pub msg: Mutex<String>,
    pub out: Mutex<PathBuf>,
    pub out_size: AtomicU64,
}

impl Job {
    fn new(src: PathBuf) -> Self {
        Self {
            name: src.file_name().unwrap_or_default().to_string_lossy().into_owned(),
            from: src.extension().unwrap_or_default().to_string_lossy().to_uppercase(),
            size: fs::metadata(&src).map_or(0, |m| m.len()),
            src,
            state: AtomicU8::new(WAIT),
            prog: AtomicU32::new(0),
            msg: Mutex::new(String::new()),
            out: Mutex::new(PathBuf::new()),
            out_size: AtomicU64::new(0),
        }
    }

    pub fn state(&self) -> u8 {
        self.state.load(Ordering::Acquire)
    }

    fn end(&self, state: u8, msg: String) {
        *self.msg.lock().unwrap() = msg;
        self.state.store(state, Ordering::Release);
    }
}

#[derive(Default)]
pub struct Queue {
    pub jobs: Vec<Arc<Job>>,
    pub out: Option<PathBuf>,
    work: Vec<JoinHandle<()>>,
    stop: Arc<AtomicBool>,
}

impl Queue {
    pub fn busy(&self) -> bool {
        self.work.iter().any(|h| !h.is_finished())
    }

    pub fn tidy(&mut self) {
        if !self.busy() {
            self.work.clear();
        }
    }

    pub fn count(&self, state: u8) -> usize {
        self.jobs.iter().filter(|j| j.state() == state).count()
    }

    pub fn left(&self) -> usize {
        self.jobs.len() - self.count(DONE)
    }

    pub fn stop(&self) {
        self.stop.store(true, Ordering::Relaxed);
    }

    pub fn close(&mut self) {
        self.stop();
        for h in self.work.drain(..) {
            let _ = h.join();
        }
    }

    pub fn add(&mut self, list: &[PathBuf], exts: &[&str]) -> usize {
        let before = self.jobs.len();
        for p in list {
            if p.is_dir() {
                let inner: Vec<PathBuf> = fs::read_dir(p).into_iter().flatten().flatten().map(|e| e.path()).filter(|p| p.is_file()).collect();
                self.add(&inner, exts);
            } else if has_ext(p, exts) && !self.jobs.iter().any(|j| j.src == *p) {
                self.jobs.push(Arc::new(Job::new(p.clone())));
            }
        }
        self.jobs.len() - before
    }

    pub fn start(&mut self, ext: &str, max: usize, task: Task, ctx: &egui::Context) {
        let mut taken = HashSet::new();
        let mut list: Vec<(Arc<Job>, PathBuf)> = Vec::new();
        for j in &self.jobs {
            if j.state() == DONE {
                continue;
            }
            j.state.store(WAIT, Ordering::Relaxed);
            j.prog.store(0, Ordering::Relaxed);
            let out = pick(&j.src, self.out.as_deref(), ext, &mut taken);
            *j.out.lock().unwrap() = out.clone();
            list.push((j.clone(), out));
        }
        if list.is_empty() {
            return;
        }
        list.sort_by_key(|(j, _)| Reverse(j.size));

        let n = thread::available_parallelism().map_or(4, |n| n.get()).saturating_sub(1).clamp(1, list.len().min(max));
        let list = Arc::new(list);
        let next = Arc::new(AtomicUsize::new(0));
        self.stop = Arc::new(AtomicBool::new(false));
        self.work = (0..n)
            .map(|_| {
                let (list, next, stop, task, ctx) = (list.clone(), next.clone(), self.stop.clone(), task.clone(), ctx.clone());
                thread::spawn(move || {
                    while let Some((j, out)) = list.get(next.fetch_add(1, Ordering::Relaxed)) {
                        if stop.load(Ordering::Relaxed) {
                            break;
                        }
                        j.state.store(WORK, Ordering::Release);
                        let tmp = part(out);
                        let res = panic::catch_unwind(AssertUnwindSafe(|| task(&j.src, &tmp, &stop, &j.prog)))
                            .unwrap_or_else(|_| Err("could not read this file".into()))
                            .and_then(|()| Ok(fs::rename(&tmp, out)?));
                        match res {
                            Ok(()) => {
                                j.out_size.store(fs::metadata(out).map_or(0, |m| m.len()), Ordering::Relaxed);
                                j.end(DONE, String::new());
                            }
                            Err(e) => {
                                let _ = fs::remove_file(&tmp);
                                if stop.load(Ordering::Relaxed) {
                                    j.end(STOP, String::new());
                                } else {
                                    j.end(FAIL, e.to_string());
                                }
                            }
                        }
                        ctx.request_repaint();
                    }
                })
            })
            .collect();
    }
}

fn has_ext(p: &Path, exts: &[&str]) -> bool {
    p.extension().and_then(|e| e.to_str()).is_some_and(|e| exts.iter().any(|x| e.eq_ignore_ascii_case(x)))
}

fn part(out: &Path) -> PathBuf {
    let mut s = out.as_os_str().to_owned();
    s.push(".part");
    s.into()
}

fn pick(src: &Path, dir: Option<&Path>, ext: &str, taken: &mut HashSet<String>) -> PathBuf {
    let dir = dir.or(src.parent()).unwrap_or(Path::new("."));
    let stem = src.file_stem().unwrap_or_default().to_string_lossy();
    let mut out = dir.join(format!("{stem}.{ext}"));
    let mut n = 1;
    while out.exists() || !taken.insert(out.to_string_lossy().to_lowercase()) {
        n += 1;
        out = dir.join(format!("{stem} ({n}).{ext}"));
    }
    out
}
