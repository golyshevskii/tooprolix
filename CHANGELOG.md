# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.3.7](https://github.com/golyshevskii/tooprolix/compare/v0.3.6...v0.3.7) - 2026-07-29

### Other

- simplify and harden the release contract ([#24](https://github.com/golyshevskii/tooprolix/pull/24))

## [0.3.6](https://github.com/golyshevskii/tooprolix/compare/v0.3.5...v0.3.6) - 2026-07-29

### Fixed

- audit the Python tests with /pytest-testing, and close what could not fail ([#21](https://github.com/golyshevskii/tooprolix/pull/21))

## [0.3.5](https://github.com/golyshevskii/tooprolix/compare/v0.3.4...v0.3.5) - 2026-07-29

### Fixed

- take the branch evidence from the API, not from the squash body
- grade the subject that lands, not only the title before the merge

### Other

- cut the release-contract gate to the part config cannot replace
- gate the PR title and document what decides the version

## [0.3.4](https://github.com/golyshevskii/tooprolix/compare/v0.3.3...v0.3.4) - 2026-07-29

### Other

- Audit the Rust code with /rust-skills, and close what it found ([#17](https://github.com/golyshevskii/tooprolix/pull/17))

## [0.3.3](https://github.com/golyshevskii/tooprolix/compare/v0.3.2...v0.3.3) - 2026-07-29

### Other

- measure coverage in both languages, and make the numbers trustworthy ([#15](https://github.com/golyshevskii/tooprolix/pull/15))

## [0.3.2](https://github.com/golyshevskii/tooprolix/compare/v0.3.1...v0.3.2) - 2026-07-29

### Added

- add `--version` and `--rules` to the discovery surface ([#13](https://github.com/golyshevskii/tooprolix/pull/13))

## [0.3.1](https://github.com/golyshevskii/tooprolix/compare/v0.3.0...v0.3.1) - 2026-07-28

### Added

- address findings by line range and announce a clean run ([#11](https://github.com/golyshevskii/tooprolix/pull/11))

## [0.3.0](https://github.com/golyshevskii/tooprolix/compare/v0.2.1...v0.3.0) - 2026-07-28

### Added

- [**breaking**] report findings from a tree that was not read whole ([#9](https://github.com/golyshevskii/tooprolix/pull/9))

## [0.2.1](https://github.com/golyshevskii/tooprolix/compare/v0.2.0...v0.2.1) - 2026-07-28

### Added

- add the `exclude` key to [tool.tooprolix] ([#7](https://github.com/golyshevskii/tooprolix/pull/7))

## [0.2.0](https://github.com/golyshevskii/tooprolix/compare/v0.1.0...v0.2.0) - 2026-07-27

### Added

- [**breaking**] replace the opt-out marker with `# !TPX00N` ([#5](https://github.com/golyshevskii/tooprolix/pull/5))

## [0.1.0](https://github.com/golyshevskii/tooprolix/releases/tag/v0.1.0) - 2026-07-27

### Added

- gate pyo3 behind a feature and ship the tooprolix command

### Fixed

- read the binary cargo reports, not a hardcoded target path
- make the AC1 and install-smoke guards grade the artifact

### Other

- settle the volume unit on words
- tooprolix 0.1.0: a prose-volume and duplicate-prose linter for Python
