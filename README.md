# cw - Count Words

A `wc` clone in Rust.


## Synopsis

```
cw 0.1.0
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
```


## Performance

It's quite fast.  Line counts are optimized using the `bytecount` crate:

```
-% dd if=pwned-passwords-1.0.txt of=/dev/null bs=32k
392544+1 records in
392544+1 records out
12862899504 bytes transferred in 21.437070 secs (600030675 bytes/sec)
21.440 real, 0.173 user, 21.264 sys

-% wc -l pwned-passwords-1.0.txt
 306259512 pwned-passwords-1.0.txt
39.252 real, 18.679 user, 20.569 sys

-% cw -l pwned-passwords-1.0.txt
 306259512 pwned-passwords-1.0.txt
21.935 real, 1.070 user, 20.857 sys
```

Other counts are probably faster because there's no multibyte handling by default:

```
-% wc pwned-passwords-1.0.txt
 306259512 306259512 12862899504 pwned-passwords-1.0.txt
1:57.72 real, 1:37.12 user, 20.592 sys

-% cw pwned-passwords-1.0.txt
 306259512 306259512 12862899504 pwned-passwords-1.0.txt
1:03.70 real, 42.798 user, 20.899 sys
```

But even using UTF-8 processing it's not bad:

```
-% wc -mLlw pwned-passwords-1.0.txt
 306259512 306259512 12862899504      41 pwned-passwords-1.0.txt
5:53.70 real, 5:32.75 user, 20.920 sys

-% cw -mLlw pwned-passwords-1.0.txt
 306259512 306259512 12862899504      41 pwned-passwords-1.0.txt
2:15.46 real, 1:54.45 user, 21.008 sys
```

For best results build with:

```
cargo build --release --features runtime-dispatch-simd
```

This enables SIMD optimizations for line counting.  It has no affect if you have
it count anything else.
