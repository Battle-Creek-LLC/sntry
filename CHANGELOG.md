# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.2.0] — 2026-05-22

### Added

- `sntry metrics query <NAME>` for querying Sentry's **Trace Metrics**
  (trace-connected Application Metrics) dataset — custom counters, gauges,
  and distributions emitted via the SDK metrics API
  (`sentry_sdk.metrics.count` / `.gauge` / `.distribution`). Wraps the
  Discover events endpoint with `dataset=tracemetrics`. Supports `--stat`
  (`sum`/`avg`/`count`/`count_unique`/`min`/`max`/`p50`..`p99`), repeatable
  `--group-by`, an extra `--query` attribute filter, and the same time-range /
  output ergonomics as `discover query`. Trace-metric aggregates require the
  full `(name, type, unit)` triple — `sntry` auto-detects the metric's `--type`
  and `--unit` from its name (both remain available as explicit overrides), so
  the common case is just `sntry metrics query <NAME> --stat sum --group-by <attr>`.

## [0.1.1] — 2026-05-09

### Changed

- `sntry issues list` now renders a `WHERE` column (top-frame
  `metadata.function` / `metadata.filename`, falling back to `culprit`)
  in place of `TITLE` for sweep / monitor flows. Pass `--full` to
  restore the original `TITLE` column. JSON / NDJSON output is
  unchanged.

### Fixed

- Cross-compilation to Windows. `config.rs` referenced
  `std::os::unix::fs::PermissionsExt` unconditionally, breaking the
  `x86_64-pc-windows-msvc` target in CI. The Unix-only file-mode check
  and `0600`-on-write are now gated with `#[cfg(unix)]` (matching the
  pattern used by `bcl-sumo`).

## [0.1.0] — 2026-05-09

### Added

- Initial release.
- `sntry auth {login,logout,use,list,status}` for managing credential
  profiles in a TOML config file (default `~/.config/sntry/config.toml`,
  mode `0600` enforced; refuses to read a world- or group-readable file).
- `sntry config {path,show}` for inspecting the resolved config.
- `sntry orgs list`, `sntry projects {list,get}` for organization and
  project discovery.
- `sntry issues {list,get,events,update}` covering the incident-triage
  surface. `update` is gated behind `--yes`.
- `sntry events get` for fetching full event payloads (stack trace,
  breadcrumbs, contexts).
- `sntry releases {list,get}` against the Releases API.
- `sntry discover query` for running Discover events queries.
- `sntry tail` for polling Discover for new matching events with event-ID
  deduplication and SIGINT-safe summary output.
- Output formats: `text`, `json`, `ndjson`, `table`. NDJSON for streaming;
  defaults to `text` on a TTY and `json` otherwise.
- Auto-pagination via the Sentry `Link` header; per-command `--max` ceiling.
- Exponential-backoff retries on `429` and `5xx` responses, honors
  `Retry-After`.
- Per-field credential resolution: CLI flag > env var
  (`SENTRY_AUTH_TOKEN`, `SENTRY_HOST`, `SENTRY_ORG`) > config file.
- Self-hosted Sentry support via the `host` field.
