use std::collections::BinaryHeap;
use std::fs::File;
use std::io::{self, BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicUsize;
use std::sync::Arc;
use structopt::StructOpt;

use crossbeam_channel;
use crossbeam_utils::thread;

use cw;
use cw::args::Opt;
use cw::count::{Counter, Counts, Strategy};
use cw::siginfo;

struct ComputedCount(usize, Result<Counts, (PathBuf, io::Error)>);

impl PartialEq for ComputedCount {
    fn eq(&self, o: &Self) -> bool {
        o.0.eq(&self.0)
    }
}
impl Eq for ComputedCount {}
impl PartialOrd for ComputedCount {
    fn partial_cmp(&self, o: &Self) -> Option<std::cmp::Ordering> {
        o.0.partial_cmp(&self.0)
    }
}
impl Ord for ComputedCount {
    fn cmp(&self, o: &Self) -> std::cmp::Ordering {
        o.0.cmp(&self.0)
    }
}

#[cfg(unix)]
fn bytes_to_pathbuf(bytes: &[u8]) -> PathBuf {
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt;

    PathBuf::from(OsStr::from_bytes(bytes).to_owned())
}

#[cfg(not(unix))]
fn bytes_to_pathbuf(bytes: &[u8]) -> PathBuf {
    // Blargh, it'll do for now, I guess :/
    PathBuf::from(String::from_utf8_lossy(&bytes).to_string())
}

fn append_delimited_filenames_read<R: Read>(
    source: R,
    dest: &mut Vec<PathBuf>,
    delimiter: u8,
) -> io::Result<()> {
    let reader = BufReader::new(source);

    for file in reader
        .split(delimiter)
        .map(|name| name.map(|n| bytes_to_pathbuf(&n)))
    {
        dest.push(file?);
    }

    Ok(())
}

fn append_delimited_filenames<P: AsRef<Path>>(
    source: P,
    mut dest: &mut Vec<PathBuf>,
    delimiter: u8,
) -> io::Result<()> {
    let source = source.as_ref();

    if source == Path::new("-") {
        append_delimited_filenames_read(&mut io::stdin(), &mut dest, delimiter)
    } else {
        append_delimited_filenames_read(File::open(source)?, &mut dest, delimiter)
    }
}

fn main() -> io::Result<()> {
    let mut opt = Opt::from_args();
    let mut total = Counts::new("total");
    let stdout = io::stdout();
    let mut out = stdout.lock();
    let mut exit_code = 0;

    siginfo::hook_signal();

    if !(opt.bytes || opt.words || opt.chars || opt.lines || opt.longest_line) {
        opt.lines = true;
        opt.bytes = true;
        opt.words = true;
    }

    if let Some(ref path) = opt.files_from {
        append_delimited_filenames(path, &mut opt.input, b'\n')?;
    }

    if let Some(ref path) = opt.files0_from {
        append_delimited_filenames(path, &mut opt.input, b'\0')?;
    }

    let strategy = Strategy::from(&opt);

    if opt.input.is_empty() {
        let mut count = Counts::default();
        strategy.count(&mut io::stdin(), &mut count, &opt)?;
        return count.print(&opt, &mut out);
    }

    let items = opt.input.len();
    let threads = std::cmp::min(items, opt.threads);

    if threads > 1 {
        thread::scope(|scope| {
            let (result_tx, result_rx) = crossbeam_channel::bounded(128);
            let count_idx = Arc::new(AtomicUsize::new(0));
            let opt = Arc::new(opt.clone());

            for _ in 0..threads {
                let result_tx = result_tx.clone();
                let count_idx = count_idx.clone();
                let opt = opt.clone();

                scope.spawn(move |_| {
                    let mut i;
                    loop {
                        i = count_idx.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                        if i >= items {
                            break;
                        }
                        let path = &opt.input[i];

                        let ret = strategy
                            .count_file(&path, &opt)
                            .map_err(|e| (path.clone(), e));

                        if result_tx.send(ComputedCount(i, ret)).is_err() {
                            break;
                        }
                    }

                    drop(result_tx);
                });
            }
            drop(result_tx);

            let mut buffered = BinaryHeap::new();
            let mut next = 0;

            for item in result_rx {
                buffered.push(item);

                while buffered.peek().map(|x| x.0) == Some(next) {
                    let ComputedCount(_, count) = buffered.pop().expect("binary heap pop");
                    next += 1;

                    match count {
                        Ok(count) => {
                            total.add(&count);
                            count.print(&opt, &mut out).expect("stdout");
                        }
                        Err((path, e)) => {
                            exit_code = 1;
                            eprintln!("{}: {}", path.display(), e);
                        }
                    }
                }
            }
        })
        .expect("thread");
    } else {
        for path in &opt.input {
            match strategy.count_file(&path, &opt) {
                Ok(count) => {
                    total.add(&count);
                    count.print(&opt, &mut out)?;
                }
                Err(e) => {
                    exit_code = 1;
                    eprintln!("{}: {}", path.display(), e);
                }
            };
        }
    }

    if opt.input.len() > 1 {
        total.print(&opt, &mut out)?;
    }

    std::process::exit(exit_code);
}
