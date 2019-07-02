use std::fs::File;
#[cfg(test)]
use std::io::Cursor;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};

use memchr::memchr_iter;

use bstr::ByteSlice;

const READ_SIZE: usize = 1024 * 32;

use crate::args::Opt;
use crate::siginfo;

#[derive(Debug, Default)]
pub struct Counts {
    pub path: Option<PathBuf>,
    pub lines: u64,
    pub words: u64,
    pub bytes: u64,
    pub chars: u64,
    pub longest_line: u64,
}

#[derive(Debug, Default)]
pub struct Capability {
    rank: u32,
    lines: bool,
    words: bool,
    bytes: bool,
    chars: bool,
    longest_line: bool,
}

impl Counts {
    pub fn new<P: Into<PathBuf>>(path: P) -> Self {
        Self {
            path: Some(path.into()),
            ..Self::default()
        }
    }

    pub fn add(&mut self, other: &Counts) {
        self.lines += other.lines;
        self.words += other.words;
        self.bytes += other.bytes;
        self.chars += other.chars;
        self.longest_line = std::cmp::max(self.longest_line, other.longest_line);
    }

    pub fn print<W: Write>(&self, opt: &Opt, mut out: W) -> io::Result<()> {
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

impl Capability {
    fn is_compatible(&self, opt: &Opt) -> bool {
        (!opt.lines || self.lines)
            && (!opt.bytes || self.bytes)
            && (!opt.chars || self.chars)
            && (!opt.words || (self.words && self.chars == opt.chars))
            && (!opt.longest_line || (self.longest_line && self.chars == opt.chars))
    }
}

macro_rules! counter_strategies {
    ($($name:ident,)+) => {
        #[derive(Debug, Clone, Copy)]
        pub enum Strategy {
            $($name,)+
        }

        impl From<&Opt> for Strategy {
            fn from(opt: &Opt) -> Self {
                let strategies = [
                    $((Strategy::$name, $name.capabilities()),)+
                ];

                strategies
                    .iter()
                    .filter(|(_, cap)| cap.is_compatible(&opt))
                    .min_by(|(_, a), (_, b)| a.rank.cmp(&b.rank))
                    .map(|(strat, _)| *strat)
                    .expect("[BUG] Unable to find a suitable implementation")
            }
        }

        impl Counter for Strategy {
            fn capabilities(&self) -> Capability {
                match self {
                    $(Strategy::$name => $name.capabilities(),)+
                }
            }

            fn count<R: Read>(&self, r: R, mut count: &mut Counts, opt: &Opt) -> io::Result<()> {
                match self {
                    $(Strategy::$name => $name.count(r, &mut count, &opt),)+
                }
            }

            fn count_file<F: AsRef<Path>>(&self, path: F, opt: &Opt) -> io::Result<Counts> {
                match self {
                    $(Strategy::$name => $name.count_file(path, &opt),)+
                }
            }
        }
    }
}

counter_strategies! {
    BytesOnly,
    LinesOnly,
    CharsOnly,
    LinesLongest,
    WordsLinesLongest,
    CharsLinesLongest,
    CharsWordsLinesLongest,
}

pub trait Counter {
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

struct BytesOnly;
impl Counter for BytesOnly {
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
            .map(std::fs::Metadata::len)
            .next();

        if let Some(bytes) = bytes {
            count.bytes = bytes;
        } else {
            File::open(&path).and_then(|fd| self.count(fd, &mut count, &opt))?;
        }

        Ok(count)
    }

    // Null counting: just let the macro count read() bytes
    fn_count!(|| |_buf: &[u8], _count: &mut Counts| { /* ... */ });
}

#[test]
fn test_bytes() {
    let mut c = Counts::default();
    BytesOnly
        .count(Cursor::new(b"12345678"), &mut c, &Opt::default())
        .unwrap();
    assert_eq!(c.bytes, 8);
}

struct LinesOnly;
impl Counter for LinesOnly {
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

#[test]
fn test_lines() {
    let mut c = Counts::default();
    LinesOnly
        .count(Cursor::new(b"\n\n\n\n\n\n\n\n"), &mut c, &Opt::default())
        .unwrap();
    assert_eq!(c.lines, 8);
}

struct CharsOnly;
impl Counter for CharsOnly {
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

#[test]
fn test_chars() {
    let mut c = Counts::default();
    CharsOnly
        .count(Cursor::new(b"fo\xC3\xB3"), &mut c, &Opt::default())
        .unwrap();
    assert_eq!(c.chars, 3);
    assert_eq!(c.bytes, 4);
}

struct LinesLongest;
impl Counter for LinesLongest {
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

#[test]
fn test_lines_longest() {
    let mut c = Counts::default();
    LinesLongest
        .count(
            Cursor::new(b"foo\nbar\nmoooo\nhmm\n"),
            &mut c,
            &Opt::default(),
        )
        .unwrap();
    assert_eq!(c.lines, 4);
    assert_eq!(c.longest_line, 5);
}

struct WordsLinesLongest;
impl Counter for WordsLinesLongest {
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

#[test]
fn test_words_lines_longest() {
    let mut c = Counts::default();
    WordsLinesLongest
        .count(
            Cursor::new(b"one two\nthree\nfour five six\n"),
            &mut c,
            &Opt::default(),
        )
        .unwrap();
    assert_eq!(c.lines, 3);
    assert_eq!(c.words, 6);
    assert_eq!(c.longest_line, 13);
}

struct CharsLinesLongest;
impl Counter for CharsLinesLongest {
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

#[test]
fn test_chars_lines_longest() {
    let mut c = Counts::default();
    CharsLinesLongest
        .count(
            Cursor::new(b"foo\nbar\nmoo\xC3\xB3o\nhmm\n"),
            &mut c,
            &Opt::default(),
        )
        .unwrap();
    assert_eq!(c.lines, 4);
    assert_eq!(c.chars, c.bytes - 1);
    assert_eq!(c.longest_line, 5);
}

struct CharsWordsLinesLongest;
impl Counter for CharsWordsLinesLongest {
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
        //
        // We limit reads to READ_SIZE to place an upper-bound on memory use.
        let mut buf = Vec::with_capacity(READ_SIZE);
        while reader.by_ref().take(READ_SIZE as u64).read_until(b'\n', &mut buf)? > 0 {
            count.bytes += buf.len() as u64;
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

#[test]
fn test_chars_words_lines_longest() {
    let mut c = Counts::default();
    CharsWordsLinesLongest
        .count(
            Cursor::new(b"\xC3\xB3ne two\nthree\nfour five six\n"),
            &mut c,
            &Opt::default(),
        )
        .unwrap();
    assert_eq!(c.lines, 3);
    assert_eq!(c.words, 6);
    assert_eq!(c.chars, c.bytes - 1);
    assert_eq!(c.longest_line, 13);
}
