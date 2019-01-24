use std::collections::BinaryHeap;
use std::fs::File;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicUsize;
use std::sync::Arc;

use crossbeam_channel;
use crossbeam_utils::thread;
use memchr::memchr_iter;
use structopt::StructOpt;

const READ_SIZE: usize = 1024 * 32;

mod siginfo;

#[derive(Debug, StructOpt, Clone)]
#[structopt(
    name = "cw",
    about = "Count Words - word, line, character and byte count"
)]
struct Opt {
    /// Count lines
    #[structopt(short = "l", long = "lines")]
    lines: bool,
    /// Count words
    #[structopt(short = "w", long = "words")]
    words: bool,
    /// Count bytes
    #[structopt(short = "c", long = "bytes", overrides_with = "chars", multiple=true)]
    bytes: bool,
    /// Count bytes (default) or characters (-m) of the longest line
    #[structopt(short = "L", long = "max-line-length")]
    longest_line: bool,
    /// Count UTF-8 characters instead of bytes
    #[structopt(short = "m", long = "chars", overrides_with = "bytes", multiple=true)]
    chars: bool,
    /// Number of counting threads to spawn
    #[structopt(long = "threads", default_value = "1")]
    threads: usize,
    /// Read input from the newline-terminated list of filenames in the given file.
    #[structopt(long = "files-from")]
    files_from: Option<PathBuf>,
    /// Read input from the NUL-terminated list of filenames in the given file.
    #[structopt(long = "files0-from")]
    files0_from: Option<PathBuf>,
    /// Input files
    #[structopt(parse(from_os_str))]
    input: Vec<PathBuf>,
}

#[derive(Debug, Default)]
struct Counts {
    path: Option<PathBuf>,
    lines: u64,
    words: u64,
    bytes: u64,
    chars: u64,
    longest_line: u64,
}

#[derive(Debug, Default)]
struct Capability {
    rank: u32,
    lines: bool,
    words: bool,
    bytes: bool,
    chars: bool,
    longest_line: bool,
}

impl Capability {
    fn is_compatible(&self, opt: &Opt) -> bool {
        (!opt.lines || self.lines)
            && (!opt.bytes || self.bytes)
            && (!opt.chars || self.chars)
            && (!opt.words || (self.words && self.chars == opt.chars))
            && (!opt.longest_line || (self.longest_line && self.chars == opt.chars))
    }
}

#[derive(Clone, Copy)]
enum Strategy {
    BytesOnly(CountBytesOnly),
    LinesOnly(CountLinesOnly),
    CharsOnly(CountCharsOnly),
    LinesLongest(CountLinesOnly),
    WordsLinesLongest(CountWordsLinesLongest),
    CharsLinesLongest(CountCharsLinesLongest),
    CharsWordsLinesLongest(CountCharsWordsLinesLongest),
}

impl Strategy {
    fn select(opt: &Opt) -> Self {
        let strategies = [
            Strategy::BytesOnly(CountBytesOnly),
            Strategy::LinesOnly(CountLinesOnly),
            Strategy::CharsOnly(CountCharsOnly),
            Strategy::LinesLongest(CountLinesOnly),
            Strategy::WordsLinesLongest(CountWordsLinesLongest),
            Strategy::CharsLinesLongest(CountCharsLinesLongest),
            Strategy::CharsWordsLinesLongest(CountCharsWordsLinesLongest),
        ];

        strategies
            .iter()
            .map(|s| (s, s.capabilities()))
            .filter(|(_, cap)| cap.is_compatible(&opt))
            .min_by(|(_, a), (_, b)| a.rank.cmp(&b.rank))
            .map(|(strat, _)| *strat)
            .expect("[BUG] Unable to find a suitable implementation")
    }
}

impl Counter for Strategy {
    fn capabilities(&self) -> Capability {
        match self {
            Strategy::BytesOnly(strat) => strat.capabilities(),
            Strategy::LinesOnly(strat) => strat.capabilities(),
            Strategy::CharsOnly(strat) => strat.capabilities(),
            Strategy::LinesLongest(strat) => strat.capabilities(),
            Strategy::WordsLinesLongest(strat) => strat.capabilities(),
            Strategy::CharsLinesLongest(strat) => strat.capabilities(),
            Strategy::CharsWordsLinesLongest(strat) => strat.capabilities(),
        }
    }

    fn count<R: Read>(&self, r: R, mut count: &mut Counts, opt: &Opt) -> io::Result<()> {
        match self {
            Strategy::BytesOnly(strat) => strat.count(r, &mut count, &opt),
            Strategy::LinesOnly(strat) => strat.count(r, &mut count, &opt),
            Strategy::CharsOnly(strat) => strat.count(r, &mut count, &opt),
            Strategy::LinesLongest(strat) => strat.count(r, &mut count, &opt),
            Strategy::WordsLinesLongest(strat) => strat.count(r, &mut count, &opt),
            Strategy::CharsLinesLongest(strat) => strat.count(r, &mut count, &opt),
            Strategy::CharsWordsLinesLongest(strat) => strat.count(r, &mut count, &opt),
        }
    }

    fn count_file<F: AsRef<Path>>(&self, path: F, opt: &Opt) -> io::Result<Counts> {
        match self {
            Strategy::BytesOnly(strat) => strat.count_file(path, &opt),
            Strategy::LinesOnly(strat) => strat.count_file(path, &opt),
            Strategy::CharsOnly(strat) => strat.count_file(path, &opt),
            Strategy::LinesLongest(strat) => strat.count_file(path, &opt),
            Strategy::WordsLinesLongest(strat) => strat.count_file(path, &opt),
            Strategy::CharsLinesLongest(strat) => strat.count_file(path, &opt),
            Strategy::CharsWordsLinesLongest(strat) => strat.count_file(path, &opt),
        }
    }
}

trait Counter {
    fn capabilities(&self) -> Capability;

    fn count<R: Read>(&self, r: R, count: &mut Counts, opt: &Opt) -> io::Result<()>;

    fn count_file<F: AsRef<Path>>(&self, path: F, opt: &Opt) -> io::Result<Counts> {
        let path = path.as_ref();
        let mut count = Counts::new(path);

        File::open(&path).and_then(|fd| self.count(fd, &mut count, &opt))?;
        Ok(count)
    }
}

macro_rules! fn_count {
    ($counter:expr) => {
        fn count<R: Read>(&self, r: R, count: &mut Counts, opt: &Opt) -> io::Result<()> {
            let mut reader = BufReader::with_capacity(READ_SIZE, r);
            #[allow(unused_mut)]
            let mut counter = $counter();

            loop {
                let len = {
                    let buf = reader.fill_buf()?;
                    if buf.is_empty() {
                        break;
                    }
                    counter(&buf, count);

                    buf.len()
                };
                count.bytes += len as u64;
                reader.consume(len);

                if siginfo::check_signal() {
                    let err = io::stderr();
                    let mut errl = err.lock();
                    let _ = count.print(&opt, &mut errl);
                }
            }

            Ok(())
        }
    };
}


#[derive(Clone, Copy)]
struct CountBytesOnly;
impl Counter for CountBytesOnly {
    fn capabilities(&self) -> Capability {
        Capability {
            rank: 0,
            bytes: true,
            ..Capability::default()
        }
    }

    // Try using stat if we only want the number of bytes
    fn count_file<F: AsRef<Path>>(&self, path: F, opt: &Opt) -> io::Result<Counts> {
        let path = path.as_ref();
        let mut count = Counts::new(path);

        let bytes = std::fs::metadata(&path)
            .iter()
            .filter(|md| md.is_file())
            .map(|md| md.len())
            .next();

        if let Some(bytes) = bytes {
            count.bytes = bytes;
        } else {
            File::open(&path).and_then(|fd| self.count(fd, &mut count, &opt))?;
        }

        Ok(count)
    }

    // Null counting: just let the macro count read() bytes
    fn_count!(|| |_buf: &[u8], _count: &mut Counts| {
        /* ... */
    });
}

#[derive(Clone, Copy)]
struct CountLinesOnly;
impl Counter for CountLinesOnly {
    fn capabilities(&self) -> Capability {
        Capability {
            rank: 1,
            bytes: true,
            lines: true,
            ..Capability::default()
        }
    }

    // Fast path for -l
    fn_count!(|| |buf: &[u8], count: &mut Counts| {
        count.lines += bytecount::count(&buf, b'\n') as u64;
    });
}

#[derive(Clone, Copy)]
struct CountCharsOnly;
impl Counter for CountCharsOnly {
    fn capabilities(&self) -> Capability {
        Capability {
            rank: 1,
            bytes: true,
            chars: true,
            ..Capability::default()
        }
    }

    // Fast path for -m
    fn_count!(|| |buf: &[u8], count: &mut Counts| {
        count.chars += bytecount::num_chars(&buf) as u64;
    });
}

#[derive(Clone, Copy)]
struct CountLinesLongest;
impl Counter for CountLinesLongest {
    fn capabilities(&self) -> Capability {
        Capability {
            rank: 30,
            bytes: true,
            lines: true,
            longest_line: true,
            ..Capability::default()
        }
    }

    // Fast path for -lL
    fn_count!(|| {
        let mut line_len = 0_u64;

        move |buf: &[u8], count: &mut Counts| {
            let mut last_pos = 0;
            for pos in memchr_iter(b'\n', buf) {
                line_len += ((pos - last_pos as usize) - 1) as u64;

                if count.longest_line < line_len {
                    count.longest_line = line_len;
                }

                line_len = 0;

                count.lines += 1;
                last_pos = pos as u64;
            }

            line_len = (buf.len() - last_pos as usize) as u64;
        }
    });
}

#[derive(Clone, Copy)]
struct CountWordsLinesLongest;
impl Counter for CountWordsLinesLongest {
    fn capabilities(&self) -> Capability {
        Capability {
            rank: 150,
            words: true,
            bytes: true,
            lines: true,
            longest_line: true,
            ..Capability::default()
        }
    }

    // Simple ASCII word count
    fn_count!(|| {
        let mut line_len = 0_u64;
        let mut in_word = false;

        move |buf: &[u8], count: &mut Counts| {
            for b in buf {
                if (*b as char).is_ascii_whitespace() {
                    in_word = false;

                    if *b == b'\n' {
                        if count.longest_line < line_len {
                            count.longest_line = line_len
                        }

                        line_len = 0;
                        count.lines += 1;
                    } else {
                        line_len += 1;
                    }
                } else {
                    if !in_word {
                        count.words += 1;
                    }
                    in_word = true;
                    line_len += 1;
                }
            }
        }
    });
}

#[derive(Clone, Copy)]
struct CountCharsLinesLongest;
impl Counter for CountCharsLinesLongest {
    fn capabilities(&self) -> Capability {
        Capability {
            rank: 120,
            bytes: true,
            chars: true,
            lines: true,
            longest_line: true,
            ..Capability::default()
        }
    }

    // Fast path for -mlL
    fn_count!(|| {
        let mut last_chars = 0;

        move |buf: &[u8], count: &mut Counts| {
            // http://canonical.org/~kragen/strlen-utf8
            //
            // Counting bytes that don't start 0b10
            for b in buf {
                if (b & 0xc0) != 0x80 {
                    count.chars += 1;

                    if *b == b'\n' {
                        let line_len = (count.chars - last_chars) - 1;
                        last_chars = count.chars;

                        if count.longest_line < line_len {
                            count.longest_line = line_len
                        }
                        count.lines += 1;
                    }
                }
            }
        }
    });
}

#[derive(Clone, Copy)]
struct CountCharsWordsLinesLongest;
impl Counter for CountCharsWordsLinesLongest {
    fn capabilities(&self) -> Capability {
        Capability {
            rank: 400,
            words: true,
            bytes: true,
            chars: true,
            lines: true,
            longest_line: true,
        }
    }

    fn count<R: Read>(&self, r: R, count: &mut Counts, opt: &Opt) -> io::Result<()> {
        let mut reader = BufReader::with_capacity(READ_SIZE, r);

        let mut line_len = 0_u64;
        let mut in_word = false;

        // Lines are useful sync points for multibyte reading
        // Could do with a mbrtowc() workalike really.
        let mut buf = String::with_capacity(READ_SIZE);
        while reader.by_ref().take(READ_SIZE as u64).read_line(&mut buf)? > 0 {
            for c in buf.chars() {
                count.chars += 1;
                if c.is_whitespace() {
                    in_word = false;

                    if c == '\n' {
                        if count.longest_line < line_len {
                            count.longest_line = line_len
                        }

                        line_len = 0;
                        count.lines += 1;
                    } else {
                        line_len += 1;
                    }
                } else {
                    if !in_word {
                        count.words += 1;
                    }
                    in_word = true;
                    line_len += 1;
                }
            }
            buf.clear();

            if siginfo::check_signal() {
                let err = io::stderr();
                let mut errl = err.lock();
                let _ = count.print(&opt, &mut errl);
            }
        }

        Ok(())
    }
}

impl Counts {
    fn new<P: Into<PathBuf>>(path: P) -> Self {
        Self {
            path: Some(path.into()),
            ..Self::default()
        }
    }

    fn add(&mut self, other: &Counts) {
        self.lines += other.lines;
        self.words += other.words;
        self.bytes += other.bytes;
        self.chars += other.chars;

        if self.longest_line < other.longest_line {
            self.longest_line = other.longest_line
        }
    }

    fn print<W: Write>(&self, opt: &Opt, mut out: W) -> io::Result<()> {
        if opt.lines {
            write!(&mut out, " {:>7}", self.lines)?;
        }

        if opt.words {
            write!(&mut out, " {:>7}", self.words)?;
        }

        if opt.chars {
            write!(&mut out, " {:>7}", self.chars)?;
        } else if opt.bytes {
            write!(&mut out, " {:>7}", self.bytes)?;
        }

        if opt.longest_line {
            write!(&mut out, " {:>7}", self.longest_line)?;
        }

        if let Some(ref path) = self.path {
            write!(&mut out, " {}", path.display())?;
        }

        writeln!(&mut out)
    }
}

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

fn append_delimited_filenames_read<R: Read>(source: R, dest: &mut Vec<PathBuf>, delimiter: u8) -> io::Result<()> {
    let reader = BufReader::new(source);

    for file in reader.split(delimiter).map(|name| name.map(|n| bytes_to_pathbuf(&n))) {
        dest.push(file?);
    }

    Ok(())
}

fn append_delimited_filenames<P: AsRef<Path>>(source: P, mut dest: &mut Vec<PathBuf>, delimiter: u8) -> io::Result<()> {
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

    let strategy = Strategy::select(&opt);

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
