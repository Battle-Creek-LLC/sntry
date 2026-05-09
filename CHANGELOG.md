# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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
