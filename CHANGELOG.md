# Changelog

All notable changes to this project will be documented in this file.
The format is based on [Keep a Changelog](http://keepachangelog.com/)
and this project adheres to [Semantic Versioning](http://semver.org/).

<!-- next-header -->

## [Unreleased]

### Changed

- Shortened description of command from `--help`.

## [0.1.6] - 2025-07-26

### Fixed

- Corrected issue with location of scoop bucket that broken windows installation.

## [0.1.5] - 2025-07-25

### Changed

- When continuing after stash was applied with conflicts and resolved, the stash is now dropped (if confirmed) after optionally committing and pushing changes.

## [0.1.4] - 2025-07-23

### Fixed

- Removed .git/sup.lock file when execution is terminated from Ctrl+C.

### Changed

- Added multiple progress bars for better visibility of operation state.

## [0.1.3] - 2025-07-21

### Fixed

- Corrected a bug when pulling changes was not possible from private repositories due to missing credentials callback.

## [0.1.2] - 2025-07-20

### Fixed

- Corrected wrong printed output when counting completed steps.

## [0.1.1] - 2025-07-20

### Fixed

- Corrected behavior when commit or push fails due to hook error and after fix, state is not empty and blocks further execution.

## [0.1.0] - 2025-07-20

### Changed

- Updated logging messages to use `debug!` instead of `info!` in several places to reduce output verbosity.
- Fancier output.

### Added

- Added support for `--message/-m` flag to commit and push stashed changes with a custom message, including hook support.

### Fixed

- Bug when after pulling changes, merge state was not created.

## [0.0.4] - 2025-07-19

### Fixed

- Fixed a bug where working directory would not contain pulled changes.

## [0.0.3] - 2025-07-19

### Fixed

- Corrected bug when wrong remote spec was used when pulling changes.

### Added

- Added support for showing version information with `--version/-v` flag.

## [0.0.2] - 2025-07-19

### Changed

- Added use of .git/sup.lock to prevent multiple executions from running simultaneously.

## [0.0.1] - 2025-07-19

### Added

- Initial release.
- Basic functionality to stash uncommitted changes, pull from remote, and restore changes.

<!-- next-url -->
[Unreleased]: https://github.com/strowk/sup/compare/v0.1.6...HEAD
[0.1.6]: https://github.com/strowk/sup/compare/v0.1.5...v0.1.6
[0.1.5]: https://github.com/strowk/sup/compare/v0.1.4...v0.1.5
[0.1.4]: https://github.com/strowk/sup/compare/v0.1.3...v0.1.4
[0.1.3]: https://github.com/strowk/sup/compare/v0.1.2...v0.1.3
[0.1.2]: https://github.com/strowk/sup/compare/v0.1.1...v0.1.2
[0.1.1]: https://github.com/strowk/sup/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/strowk/sup/compare/v0.0.4...v0.1.0
[0.0.4]: https://github.com/strowk/sup/compare/v0.0.3...v0.0.4
[0.0.3]: https://github.com/strowk/sup/compare/v0.0.2...v0.0.3
[0.0.2]: https://github.com/strowk/sup/compare/v0.0.1...v0.0.2
[0.0.1]: https://github.com/strowk/sup/releases/tag/v0.0.1
