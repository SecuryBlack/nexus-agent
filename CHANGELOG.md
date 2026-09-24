# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.15] - 2026-09-20

### Added
- **Registry**: Add local discovery and auto-installation support for `TitanVault` backup agent.
- **Registry**: Expand dynamic agent registration and status reporting for `CromoForge`.

## [0.2.14] - 2026-09-13

### Added
- **Registry**: Add discovery, health checking, and command routing for `CromoForge` container management agent.

## [0.2.13] - 2026-09-13

### Fixed
- **Windows**: Add `windows-static-crt` and PowerShell 5.1 / Windows Server 2019 compatibility fixes.
- **Registry**: Ensure binary lookup locates `.exe` extensions reliably on Windows.

## [0.2.12] - 2026-08-24

### Added
- **Commands**: Implement `update_now` command for nexus-agent itself to enable manual remote upgrades.

## [0.2.11] - 2026-08-24

### Fixed
- **Routing**: Correct `os_upgrade` socket naming by resolving target agent to socket binary name.

## [0.2.10] - 2026-08-24

### Fixed
- **CI**: Fix release tarball packaging and asset checksum generation.

## [0.2.9] - 2026-08-24

### Added
- **Tunnel**: Bi-directional command intake and command routing with `sb-agent-core` shared-token authentication.

## [0.2.8] - 2026-08-23

### Changed
- **Dependencies**: Consume `sb-agent-core` from crates.io.

## [0.2.7] - 2026-08-23

### Changed
- **Architecture**: Retrofit onto shared `sb-agent-core` runtime.

[Unreleased]: https://github.com/SecuryBlack/nexus-agent/compare/v0.2.15...HEAD
[0.2.15]: https://github.com/SecuryBlack/nexus-agent/compare/v0.2.14...v0.2.15
[0.2.14]: https://github.com/SecuryBlack/nexus-agent/compare/v0.2.13...v0.2.14
[0.2.13]: https://github.com/SecuryBlack/nexus-agent/compare/v0.2.12...v0.2.13
[0.2.12]: https://github.com/SecuryBlack/nexus-agent/compare/v0.2.11...v0.2.12
[0.2.11]: https://github.com/SecuryBlack/nexus-agent/compare/v0.2.10...v0.2.11
[0.2.10]: https://github.com/SecuryBlack/nexus-agent/compare/v0.2.9...v0.2.10
[0.2.9]: https://github.com/SecuryBlack/nexus-agent/compare/v0.2.8...v0.2.9
[0.2.8]: https://github.com/SecuryBlack/nexus-agent/compare/v0.2.7...v0.2.8
[0.2.7]: https://github.com/SecuryBlack/nexus-agent/releases/tag/v0.2.7
