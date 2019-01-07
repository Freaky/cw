use std::fs::File;
use std::io;
use std::io::BufRead;
use std::io::BufReader;
use std::io::Read;
use std::path::PathBuf;
use structopt::StructOpt;

const READ_SIZE: usize = 1024 * 32;

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

fn count_lines<R: Read>(r: R, mut total: &mut Counts) -> io::Result<Counts> {
    let mut count = Counts::default();
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
    }
    total.lines += count.lines;
    total.bytes += count.bytes;
    Ok(count)
}

fn count_bytes<R: Read>(r: R, mut total: &mut Counts) -> io::Result<Counts> {
    let mut count = Counts::default();
    let mut reader = BufReader::with_capacity(READ_SIZE, r);

    let mut line_len = 0;
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
    }

    total.bytes += count.bytes;
    total.lines += count.lines;
    total.words += count.words;

    if total.longest_line < count.longest_line {
        total.longest_line = count.longest_line;
    }

    Ok(count)
}

fn count_chars<R: Read>(r: R, mut total: &mut Counts) -> io::Result<Counts> {
    let mut count = Counts::default();
    let mut reader = BufReader::with_capacity(READ_SIZE, r);

    let mut line_len = 0;
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
    }

    total.chars += count.chars;
    total.lines += count.lines;
    total.words += count.words;

    if total.longest_line < count.longest_line {
        total.longest_line = count.longest_line;
    }

    Ok(count)
}

fn main() -> io::Result<()> {
    let mut opt = Opt::from_args();
    let mut total = Counts {
        path: Some(PathBuf::from("total")),
        ..Counts::default()
    };
    let mut exit_code = 0;

    if !(opt.bytes || opt.words || opt.chars || opt.lines || opt.longest_line) {
        opt.lines = true;
        opt.bytes = true;
        opt.words = true;
    }

    if opt.chars {
        opt.bytes = false;
    }

    let lines_only = opt.lines && !(opt.words || opt.chars || opt.longest_line);
    let bytes_only = opt.bytes && !(opt.words || opt.chars || opt.lines || opt.longest_line);

    let print_count = |cnt: &Counts| {
        if opt.lines {
            print!(" {:>7}", cnt.lines);
        }

        if opt.words {
            print!(" {:>7}", cnt.words);
        }

        if opt.chars {
            print!(" {:>7}", cnt.chars);
        } else if opt.bytes {
            print!(" {:>7}", cnt.bytes);
        }

        if opt.longest_line {
            print!(" {:>7}", cnt.longest_line);
        }

        if let Some(ref path) = cnt.path {
            print!(" {}", path.display());
        }

        println!();
    };

    if opt.input.is_empty() {
        let count = if lines_only {
            count_lines(io::stdin(), &mut total)?
        } else if opt.chars {
            count_chars(io::stdin(), &mut total)?
        } else {
            count_bytes(io::stdin(), &mut total)?
        };
        print_count(&count);
    }

    for path in &opt.input {
        if bytes_only {
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
                print_count(&count);
                continue;
            }
        }

        let count = File::open(path).and_then(|fd| {
            if lines_only {
                count_lines(fd, &mut total)
            } else if opt.chars {
                count_chars(fd, &mut total)
            } else {
                count_bytes(fd, &mut total)
            }
        });

        match count {
            Ok(mut count) => {
                count.path = Some(path.clone());
                print_count(&count);
            }
            Err(e) => {
                exit_code = 1;
                eprintln!("{:?}", e);
            }
        }
    }

    if opt.input.len() > 1 {
        print_count(&total);
    }

    std::process::exit(exit_code);
}
