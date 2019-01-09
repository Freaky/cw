# cw - Count Words

A `wc` clone in Rust.

## Synopsis

```
-% cw --help
cw 0.2.0
Thomas Hurst <tom@hur.st>
Count Words - word, line, character and byte count

USAGE:
    cw [FLAGS] [input]...

FLAGS:
    -c, --bytes              Count bytes
    -m, --chars              Count UTF-8 characters instead of bytes
    -h, --help               Prints help information
    -l, --lines              Count lines
    -L, --max-line-length    Count bytes (default) or characters (-m) of the longest line
    -V, --version            Prints version information
    -w, --words              Count words

ARGS:
    <input>...    Input files

-% cw Dickens_Charles_Pickwick_Papers.xml
 3449440 51715840 341152640 Dickens_Charles_Pickwick_Papers.xml
```

## Performance

Line counts are optimized using the `bytecount` crate:

```
Benchmark #1: wc -l Dickens_Charles_Pickwick_Papers.xml
  Time (mean ± σ):     439.7 ms ±   2.0 ms    [User: 354.9 ms, System: 84.5 ms]
  Range (min … max):   435.3 ms … 441.4 ms

Benchmark #2: gwc -l Dickens_Charles_Pickwick_Papers.xml
  Time (mean ± σ):     533.0 ms ±   1.7 ms    [User: 388.8 ms, System: 144.0 ms]
  Range (min … max):   530.9 ms … 535.1 ms

Benchmark #3: cw -l Dickens_Charles_Pickwick_Papers.xml
  Time (mean ± σ):     127.9 ms ±   1.5 ms    [User: 24.1 ms, System: 103.7 ms]
  Range (min … max):   125.1 ms … 131.3 ms

Summary
  'cw -l Dickens_Charles_Pickwick_Papers.xml' ran
    3.44 ± 0.04 times faster than 'wc -l Dickens_Charles_Pickwick_Papers.xml'
    4.17 ± 0.05 times faster than 'gwc -l Dickens_Charles_Pickwick_Papers.xml'
```

Line counts with line length are optimized using the `memchr` crate:

```
Benchmark #1: wc -lL Dickens_Charles_Pickwick_Papers.xml
  Time (mean ± σ):     441.6 ms ±   1.8 ms    [User: 354.7 ms, System: 86.5 ms]
  Range (min … max):   438.5 ms … 443.8 ms

Benchmark #2: gwc -lL Dickens_Charles_Pickwick_Papers.xml
  Time (mean ± σ):      3.851 s ±  0.005 s    [User: 3.710 s, System: 0.141 s]
  Range (min … max):    3.847 s …  3.864 s

Benchmark #3: cw -lL Dickens_Charles_Pickwick_Papers.xml
  Time (mean ± σ):     255.6 ms ±   1.1 ms    [User: 154.6 ms, System: 100.9 ms]
  Range (min … max):   253.3 ms … 256.9 ms

Summary
  'cw -lL Dickens_Charles_Pickwick_Papers.xml' ran
    1.73 ± 0.01 times faster than 'wc -lL Dickens_Charles_Pickwick_Papers.xml'
   15.07 ± 0.07 times faster than 'gwc -lL Dickens_Charles_Pickwick_Papers.xml'
```

Note without `-m` cw only operates on bytes, and it never cares about your locale.

```
Benchmark #1: wc Dickens_Charles_Pickwick_Papers.xml
  Time (mean ± σ):      2.708 s ±  0.002 s    [User: 2.612 s, System: 0.095 s]
  Range (min … max):    2.706 s …  2.712 s

Benchmark #2: gwc Dickens_Charles_Pickwick_Papers.xml
  Time (mean ± σ):      3.851 s ±  0.003 s    [User: 3.714 s, System: 0.136 s]
  Range (min … max):    3.847 s …  3.856 s

Benchmark #3: cw Dickens_Charles_Pickwick_Papers.xml
  Time (mean ± σ):      2.026 s ±  0.001 s    [User: 1.939 s, System: 0.087 s]
  Range (min … max):    2.024 s …  2.028 s

Summary
  'cw Dickens_Charles_Pickwick_Papers.xml' ran
    1.34 ± 0.00 times faster than 'wc Dickens_Charles_Pickwick_Papers.xml'
    1.90 ± 0.00 times faster than 'gwc Dickens_Charles_Pickwick_Papers.xml'
```

`-m` enables UTF-8 processing, with a fast-path for character and line length:

```
Benchmark #1: wc -mlL Dickens_Charles_Pickwick_Papers.xml
  Time (mean ± σ):      8.968 s ±  0.007 s    [User: 8.885 s, System: 0.082 s]
  Range (min … max):    8.961 s …  8.982 s

Benchmark #2: gwc -mlL Dickens_Charles_Pickwick_Papers.xml
  Time (mean ± σ):      3.847 s ±  0.006 s    [User: 3.696 s, System: 0.151 s]
  Range (min … max):    3.842 s …  3.859 s

Benchmark #3: cw -mlL Dickens_Charles_Pickwick_Papers.xml
  Time (mean ± σ):      1.073 s ±  0.001 s    [User: 971.8 ms, System: 100.6 ms]
  Range (min … max):    1.071 s …  1.075 s

Summary
  'cw -mlL Dickens_Charles_Pickwick_Papers.xml' ran
    3.59 ± 0.01 times faster than 'gwc -mlL Dickens_Charles_Pickwick_Papers.xml'
    8.36 ± 0.01 times faster than 'wc -mlL Dickens_Charles_Pickwick_Papers.xml'
```

```
Benchmark #1: wc -mlL test-utf-8.html
  Time (mean ± σ):      1.180 s ±  0.000 s    [User: 1.176 s, System: 0.010 s]
  Range (min … max):    1.180 s …  1.181 s

Benchmark #2: gwc -mlL test-utf-8.html
  Time (mean ± σ):      2.374 s ±  0.001 s    [User: 2.354 s, System: 0.017 s]
  Range (min … max):    2.372 s …  2.377 s

Benchmark #3: cw -mlL test-utf-8.html
  Time (mean ± σ):     117.4 ms ±   0.6 ms    [User: 105.9 ms, System: 10.1 ms]
  Range (min … max):   117.0 ms … 120.3 ms

  Warning: Statistical outliers were detected. Consider re-running this benchmark on a quiet PC without any interferences from other programs. It might help to use the '--warmup' or '--prepare' options.

Summary
  'cw -mlL test-utf-8.html' ran
   10.05 ± 0.05 times faster than 'wc -mlL test-utf-8.html'
   20.22 ± 0.11 times faster than 'gwc -mlL test-utf-8.html'
```

And a slow path for everything else:

```
Benchmark #1: wc -mLlw Dickens_Charles_Pickwick_Papers.xml
  Time (mean ± σ):      8.972 s ±  0.019 s    [User: 8.875 s, System: 0.096 s]
  Range (min … max):    8.958 s …  9.013 s

  Warning: Statistical outliers were detected. Consider re-running this benchmark on a quiet PC without any interferences from other programs. It might help to use the '--warmup' or '--prepare' options.

Benchmark #2: gwc -mLlw Dickens_Charles_Pickwick_Papers.xml
  Time (mean ± σ):      3.852 s ±  0.008 s    [User: 3.700 s, System: 0.151 s]
  Range (min … max):    3.846 s …  3.867 s

Benchmark #3: cw -mLlw Dickens_Charles_Pickwick_Papers.xml
  Time (mean ± σ):      3.721 s ±  0.003 s    [User: 3.598 s, System: 0.123 s]
  Range (min … max):    3.715 s …  3.726 s

Summary
  'cw -mLlw Dickens_Charles_Pickwick_Papers.xml' ran
    1.04 ± 0.00 times faster than 'gwc -mLlw Dickens_Charles_Pickwick_Papers.xml'
    2.41 ± 0.01 times faster than 'wc -mLlw Dickens_Charles_Pickwick_Papers.xml'
```

These tests are on FreeBSD 12 on a 2.1GHz Westmere Xeon.  `gwc` is from GNU
coreutils 8.30.

For best results build with:

```
cargo build --release --features runtime-dispatch-simd
```

This enables SIMD optimizations for line counting.  It has no effect if you have
it count anything else.


## Future

 * Test suite.
 * Refactor to reduce the code sprawl.
 * Improve `SIGINFO` support.
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


[uwc]: https://crates.io/crates/uwc
[rwc]: https://crates.io/crates/rwc
[linecount]: https://crates.io/crates/linecount
