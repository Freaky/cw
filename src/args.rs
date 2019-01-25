use std::path::PathBuf;
use structopt::StructOpt;

#[derive(Debug, StructOpt, Clone)]
#[structopt(
    name = "cw",
    about = "Count Words - word, line, character and byte count"
)]
pub struct Opt {
    /// Count lines
    #[structopt(short = "l", long = "lines")]
    pub lines: bool,
    /// Count words
    #[structopt(short = "w", long = "words")]
    pub words: bool,
    /// Count bytes
    #[structopt(short = "c", long = "bytes", overrides_with = "chars", multiple = true)]
    pub bytes: bool,
    /// Count bytes (default) or characters (-m) of the longest line
    #[structopt(short = "L", long = "max-line-length")]
    pub longest_line: bool,
    /// Count UTF-8 characters instead of bytes
    #[structopt(short = "m", long = "chars", overrides_with = "bytes", multiple = true)]
    pub chars: bool,
    /// Number of counting threads to spawn
    #[structopt(long = "threads", default_value = "1")]
    pub threads: usize,
    /// Read input from the newline-terminated list of filenames in the given file.
    #[structopt(long = "files-from")]
    pub files_from: Option<PathBuf>,
    /// Read input from the NUL-terminated list of filenames in the given file.
    #[structopt(long = "files0-from")]
    pub files0_from: Option<PathBuf>,
    /// Input files
    #[structopt(parse(from_os_str))]
    pub input: Vec<PathBuf>,
}
