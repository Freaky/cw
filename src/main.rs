use std::io::Write;
use memchr::memchr_iter;
use std::fs::File;
use std::io;
use std::io::BufRead;
use std::io::BufReader;
use std::io::Read;
use std::path::PathBuf;
use structopt::StructOpt;

const READ_SIZE: usize = 1024 * 32;

#[cfg(any(
    target_os = "macos",
    target_os = "ios",
    target_os = "freebsd",
    target_os = "dragonfly",
    target_os = "openbsd",
    target_os = "netbsd",
    target_os = "bitrig"
))]
mod sig {
    use libc::{c_int, c_void, sighandler_t, signal, SIGINFO};
    use std::sync::atomic::{AtomicBool, Ordering, ATOMIC_BOOL_INIT};

    static SIGINFO_RECEIVED: AtomicBool = ATOMIC_BOOL_INIT;

    extern "C" fn trigger_signal(_: c_int) {
        SIGINFO_RECEIVED.store(true, Ordering::Release);
    }

    fn get_handler() -> sighandler_t {
        trigger_signal as extern "C" fn(c_int) as *mut c_void as sighandler_t
    }

    pub fn check_signal() -> bool {
        SIGINFO_RECEIVED.swap(false, Ordering::AcqRel)
    }

    pub fn hook_signal() {
        unsafe {
            signal(SIGINFO, get_handler());
        }
    }
}

#[cfg(not(any(
    target_os = "macos",
    target_os = "ios",
    target_os = "freebsd",
    target_os = "dragonfly",
    target_os = "openbsd",
    target_os = "netbsd",
    target_os = "bitrig"
)))]
mod sig {
    pub fn check_signal() -> bool {
        false
    }

    pub fn hook_signal() {}
}

#[derive(Debug, StructOpt)]
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
    #[structopt(short = "c", long = "bytes")]
    bytes: bool,
    /// Count bytes (default) or characters (-m) of the longest line
    #[structopt(short = "L", long = "max-line-length")]
    longest_line: bool,
    /// Count UTF-8 characters instead of bytes
    #[structopt(short = "m", long = "chars")]
    chars: bool,
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

#[derive(Debug, Clone, Copy)]
enum Impl {
    Lines,      // -l, -lc (bytecount)
    LinesMax,   // -lL, -lLc (memchr)
    Codepoints, // -m, -lm, -lLm (custom UTF-8 strlen)
    BytesOnly,  // -c (fs stat or just counting read length)
    Bytes,      // no args, -w, -lw, -lwc (bytewise)
    Unicode     // -mw (String + charwise)
}

impl Default for Impl {
    fn default() -> Self {
        Impl::Bytes
    }
}

impl Impl {
    fn count<R: Read>(self, r: R, mut count: &mut Counts) -> io::Result<()> {
        match self {
            Impl::Lines | Impl::BytesOnly => count_lines(r, &mut count),
            Impl::LinesMax => count_lines_longest(r, &mut count),
            Impl::Codepoints => count_codepoints(r, &mut count),
            Impl::Bytes => count_bytes(r, &mut count),
            Impl::Unicode => count_chars(r, &mut count)
        }
    }
}

#[derive(Debug, Default)]
struct Strategy {
    id: Impl,
    rank: u32,
    lines: bool,
    words: bool,
    bytes: bool,
    chars: bool,
    longest_line: bool,
}

impl Strategy {
    fn is_usable(&self, opt: &Opt) -> bool {
        (!opt.lines || self.lines) &&
        (!opt.bytes || self.bytes) &&
        (!opt.chars || self.chars) &&
        (!opt.words || (self.words && self.chars == opt.chars)) &&
        (!opt.longest_line || (self.longest_line && self.chars == opt.chars))
    }
}

struct Strategies(Vec<Strategy>);

impl Strategies {
    fn new() -> Self {
        let mut strategies: Vec<Strategy> = vec![];

        strategies.push(Strategy {
            id: Impl::BytesOnly,
            rank: 0,
            bytes: true,
            ..Strategy::default()
        });

        strategies.push(Strategy {
            id: Impl::Lines,
            rank: 1,
            bytes: true,
            lines: true,
            ..Strategy::default()
        });

        strategies.push(Strategy {
            id: Impl::LinesMax,
            rank: 3,
            bytes: true,
            lines: true,
            longest_line: true,
            ..Strategy::default()
        });

        strategies.push(Strategy {
            id: Impl::Codepoints,
            rank: 6,
            chars: true,
            bytes: true,
            lines: true,
            longest_line: true,
            ..Strategy::default()
        });

        strategies.push(Strategy {
            id: Impl::Bytes,
            rank: 100,
            words: true,
            bytes: true,
            lines: true,
            longest_line: true,
            ..Strategy::default()
        });

        strategies.push(Strategy {
            id: Impl::Unicode,
            rank: 1000,
            words: true,
            bytes: true,
            chars: true,
            lines: true,
            longest_line: true,
        });

        Strategies(strategies)
    }

    fn select(&self, opt: &Opt) -> Impl {
        self.0
            .iter()
            .filter(|s| s.is_usable(&opt))
            .min_by(|a,b| a.rank.cmp(&b.rank))
            .map(|s| s.id)
            .expect("[BUG] Unable to find a suitable implementation")
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

// Count lines and bytes
// Fastest approach: just use an optimized bytecount for \n
//
// ~2x faster than count_lines_longest
fn count_lines<R: Read>(r: R, count: &mut Counts) -> io::Result<()> {
    let mut reader = BufReader::with_capacity(READ_SIZE, r);
    loop {
        let len = {
            let buf = reader.fill_buf()?;
            if buf.is_empty() {
                break;
            }
            count.lines += bytecount::count(&buf, b'\n') as u64;
            buf.len()
        };
        count.bytes += len as u64;
        reader.consume(len);

        if sig::check_signal() {
            eprintln!("{:?}", count);
        }
    }

    Ok(())
}

// Count lines, line length, and bytes
// Use memchr to find newlines
//
// ~9x faster than count_bytes
fn count_lines_longest<R: Read>(r: R, count: &mut Counts) -> io::Result<()> {
    let mut reader = BufReader::with_capacity(READ_SIZE, r);

    let mut line_len = 0_u64;

    loop {
        let len = {
            let buf = reader.fill_buf()?;
            if buf.is_empty() {
                break;
            }

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

            buf.len()
        };
        count.bytes += len as u64;
        reader.consume(len);

        if sig::check_signal() {
            eprintln!("{:?}", count);
        }
    }

    Ok(())
}

// Count everything, but only using bytes
//
// 1.8x faster than count_chars
fn count_bytes<R: Read>(r: R, count: &mut Counts) -> io::Result<()> {
    let mut reader = BufReader::with_capacity(READ_SIZE, r);

    let mut line_len = 0_u64;
    let mut in_word = false;

    loop {
        let len = {
            let buf = reader.fill_buf()?;
            if buf.is_empty() {
                break;
            }

            count.bytes += buf.len() as u64;

            for byte in buf {
                if (*byte as char).is_ascii_whitespace() {
                    in_word = false;
                } else {
                    if !in_word {
                        count.words += 1;
                    }
                    in_word = true;
                }

                if *byte == b'\n' {
                    if count.longest_line < line_len {
                        count.longest_line = line_len
                    }

                    line_len = 0;
                    count.lines += 1;
                } else {
                    line_len += 1;
                }
            }
            buf.len()
        };

        reader.consume(len);

        if sig::check_signal() {
            eprintln!("{:?}", count);
        }
    }

    Ok(())
}

// Count UTF-8 codepoints
fn count_codepoints<R: Read>(r: R, count: &mut Counts) -> io::Result<()> {
    let mut reader = BufReader::with_capacity(READ_SIZE, r);

    let mut last_chars = 0;

    loop {
        let len = {
            let buf = reader.fill_buf()?;
            if buf.is_empty() {
                break;
            }

            // http://canonical.org/~kragen/strlen-utf8
            //
            // Counting bytes that don't start 0b10
            for b in buf {
                if (b & 0xc0) != 0x80 {
                    count.chars += 1;

                    if *b == b'\n' {
                        let line_len = count.chars - last_chars;
                        last_chars = count.chars;

                        if count.longest_line < line_len {
                            count.longest_line = line_len
                        }
                        count.lines += 1;
                    }
                }
            }

            buf.len()
        };
        count.bytes += len as u64;

        reader.consume(len);

        if sig::check_signal() {
            eprintln!("{:?}", count);
        }
    }

    Ok(())
}

// Slow path: UTF-8 processing and additional copying on top.
fn count_chars<R: Read>(r: R, count: &mut Counts) -> io::Result<()> {
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
            } else {
                if !in_word {
                    count.words += 1;
                }
                in_word = true;
            }

            if c == '\n' {
                if count.longest_line < line_len {
                    count.longest_line = line_len
                }

                line_len = 0;
                count.lines += 1;
            } else {
                line_len += 1;
            }
        }
        buf.clear();

        if sig::check_signal() {
            println!("{:?}", count);
        }
    }

    Ok(())
}

fn main() -> io::Result<()> {
    let mut opt = Opt::from_args();
    let mut total = Counts::new("total");
    let stdout = io::stdout();
    let mut out = stdout.lock();
    let mut exit_code = 0;

    sig::hook_signal();

    if !(opt.bytes || opt.words || opt.chars || opt.lines || opt.longest_line) {
        opt.lines = true;
        opt.bytes = true;
        opt.words = true;
    }

    if opt.chars {
        opt.bytes = false;
    }

    let strategies = Strategies::new();
    let strategy = strategies.select(&opt);
    eprintln!("Strategy: {:?}", strategy);

    if opt.input.is_empty() {
        let mut count = Counts::default();
        strategy.count(&mut io::stdin(), &mut count)?;
        count.print(&opt, &mut out)?;
    }

    for path in &opt.input {
        if let Impl::BytesOnly = strategy {
            let count = std::fs::metadata(path)
                .iter()
                .filter(|md| md.is_file())
                .map(|md| Counts {
                    bytes: md.len(),
                    path: Some(path.clone()),
                    ..Counts::default()
                })
                .next();

            if let Some(count) = count {
                total.bytes += count.bytes;
                count.print(&opt, &mut out)?;
                continue;
            }
        }

        let mut count = Counts::new(path.clone());
        let success = File::open(path).and_then(|fd| strategy.count(fd, &mut count));

        match success {
            Ok(()) => {
                total.add(&count);
                count.print(&opt, &mut out)?;
            }
            Err(e) => {
                exit_code = 1;
                eprintln!("{}: {}", path.display(), e);
            }
        }
    }

    if opt.input.len() > 1 {
        total.print(&opt, &mut out)?;
    }

    std::process::exit(exit_code);
}
