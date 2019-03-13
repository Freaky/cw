# Changelog

## [0.6.0] - 2019-03-13
### Added
- Some basic tests.

### Changed
- Hook up `SIGINFO` handler to `SIGUSR1` on inferior Unix-like platforms.
- Split into library and binary (#1)

### Fixed
- Multiple `-m` and `-c` arguments.

## [0.5.0] - 2019-01-18
### Added
- This `CHANGELOG.md`.
- `--threads` support, defaulting to off.
- `--files0-from` and `--files-from`, similar to GNU wc.

### Changed
- ~15% performance bump for slow `-w` and `-mw` paths.
- `-m` and `-c` now toggle each other.


## [0.4.0] - 2019-01-13
### Added
- Fast path for `-c` when `stat()` can't be used.
- Faster path for `-m` using `bytecount`.


### Changed
- Significant improvements to code path selection.


## [0.3.0] - 2019-01-09
### Added
- Fast path for `-m` and `-mL`.

### Changed
- Complete `SIGINFO` support.


## [0.2.0] - 2019-01-07
### Added
 - Fast path for `-L` using `memchr`.
 - Initial `SIGINFO` support.


## [0.1.0] - 2019-01-06
### Added
 - Initial release.


[0.6.0]: https://github.com/Freaky/cw/releases/tag/v0.6.0
[0.5.0]: https://github.com/Freaky/cw/releases/tag/v0.5.0
[0.4.0]: https://github.com/Freaky/cw/releases/tag/v0.4.0
[0.3.0]: https://github.com/Freaky/cw/releases/tag/v0.3.0
[0.2.0]: https://github.com/Freaky/cw/releases/tag/v0.2.0
[0.1.0]: https://github.com/Freaky/cw/releases/tag/v0.1.0
