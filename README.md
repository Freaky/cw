# cw - Count Words

A fast `wc` clone in Rust.

## Synopsis

```
-% cw --help
cw 0.5.0
Thomas Hurst <tom@hur.st>
Count Words - word, line, character and byte count

USAGE:
    cw [FLAGS] [OPTIONS] [input]...

FLAGS:
    -c, --bytes              Count bytes
    -m, --chars              Count UTF-8 characters instead of bytes
    -h, --help               Prints help information
    -l, --lines              Count lines
    -L, --max-line-length    Count bytes (default) or characters (-m) of the longest line
    -V, --version            Prints version information
    -w, --words              Count words

OPTIONS:
        --threads <threads>    Number of counting threads to spawn [default: 1]

ARGS:
    <input>...    Input files

-% cw Dickens_Charles_Pickwick_Papers.xml
 3449440 51715840 341152640 Dickens_Charles_Pickwick_Papers.xml
```

## Performance

Counts of multiple files may be accelerated by use of the `--threads` option:

```
  'xargs <files cw --threads=12' ran
    2.01 ± 0.03 times faster than 'xargs <files cw --threads=4'
    7.07 ± 0.09 times faster than 'xargs <files cw'
   11.55 ± 0.15 times faster than 'xargs <files wc'
   17.31 ± 0.23 times faster than 'xargs <files gwc'
```

Line counts are optimized using the [`bytecount`][bytecount] crate:

```
  'cw -l Dickens_Charles_Pickwick_Papers.xml' ran
    3.44 ± 0.04 times faster than 'wc -l Dickens_Charles_Pickwick_Papers.xml'
    4.17 ± 0.05 times faster than 'gwc -l Dickens_Charles_Pickwick_Papers.xml'
```

Line counts with line length are optimized using the [`memchr`][memchr] crate:

```
  'cw -lL Dickens_Charles_Pickwick_Papers.xml' ran
    1.73 ± 0.01 times faster than 'wc -lL Dickens_Charles_Pickwick_Papers.xml'
   15.07 ± 0.07 times faster than 'gwc -lL Dickens_Charles_Pickwick_Papers.xml'
```

Note without `-m` cw only operates on bytes, and it never cares about your locale.

```
  'cw Dickens_Charles_Pickwick_Papers.xml' ran
    1.45 ± 0.01 times faster than 'wc Dickens_Charles_Pickwick_Papers.xml'
    2.05 ± 0.00 times faster than 'gwc Dickens_Charles_Pickwick_Papers.xml'
```

`-m` enables UTF-8 processing, with a fast-path for just character length, again
using `bytecount`:

```
  'cw -m Dickens_Charles_Pickwick_Papers.xml' ran
   30.21 ± 0.39 times faster than 'gwc -m Dickens_Charles_Pickwick_Papers.xml'
   70.36 ± 0.91 times faster than 'wc -m Dickens_Charles_Pickwick_Papers.xml'
```

```
  'cw -m test-utf-8.html' ran
   84.74 ± 1.12 times faster than 'wc -m test-utf-8.html'
  124.21 ± 1.64 times faster than 'gwc -m test-utf-8.html'
```

And another path for character and line length:

```
  'cw -mlL Dickens_Charles_Pickwick_Papers.xml' ran
    3.88 ± 0.01 times faster than 'gwc -mlL Dickens_Charles_Pickwick_Papers.xml'
    9.05 ± 0.02 times faster than 'wc -mlL Dickens_Charles_Pickwick_Papers.xml'
```

```
  'cw -mlL test-utf-8.html' ran
    9.42 ± 0.01 times faster than 'wc -mlL test-utf-8.html'
   18.95 ± 0.03 times faster than 'gwc -mlL test-utf-8.html'
```

And a slow path for everything else:

```
  'cw -mLlw Dickens_Charles_Pickwick_Papers.xml' ran
    1.35 ± 0.00 times faster than 'gwc -mLlw Dickens_Charles_Pickwick_Papers.xml'
    3.15 ± 0.00 times faster than 'wc -mLlw Dickens_Charles_Pickwick_Papers.xml'
```

These tests are on FreeBSD 12 on a 2.1GHz Westmere Xeon.  `gwc` is from GNU
coreutils 8.30 - note its performance here is rather pessimised in some areas by
FreeBSD's rather weak `memchr` implementation.  YMMV.

For best results build with:

```
cargo build --release --features runtime-dispatch-simd
```

This enables SIMD optimizations for line and character counting.  It has no
effect if you count anything else.


## Future

 * Test suite.
 * Factor internals out into a library. (#1)
 * Improve multibyte support.
 * Possibly implement locale.
 * Replace clap/structopt with something lighter.


## See Also

### [uwc]

[uwc] focuses on following Unicode rules as precisely as possible, taking into
account less-common newlines, counting graphemes as well as codepoints, and
following Unicode word-boundary rules precisely.

The cost of this is currently a great deal of performance, with counts on my
benchmark file taking over a minute.


### [rwc]

cw was originally called [rwc] until I noticed this existed.  It's quite old and
doesn't appear to compile.


### [linecount]

A little library that only does plain newline counting, along with a binary
called `lc`.  Version 0.2 will use the same algorithm as `cw`.


[bytecount]: https://crates.io/crates/bytecount
[memchr]: https://crates.io/crates/memchr
[uwc]: https://crates.io/crates/uwc
[rwc]: https://crates.io/crates/rwc
[linecount]: https://crates.io/crates/linecount
