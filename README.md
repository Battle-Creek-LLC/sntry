# sntry

A fast, minimal CLI for investigating [Sentry](https://sentry.io/) issues,
events, and releases from the terminal.

```
$ sntry issues list 'is:unresolved level:error' --from now-1h
SHORT_ID         LEVEL  COUNT  USERS  TITLE
ACME-API-1A2     error  1,204  187    DatabaseError: connection lost
ACME-API-1A3     error    412   53    TimeoutError: upstream took >30s
```

Distinct from the official `sentry-cli` — that tool focuses on uploading
release artifacts and source maps. `sntry` is a read-side companion for
incident triage and ad-hoc queries.

## Features

- **Issues + events + Discover** — list issues, fetch full event payloads,
  run Discover queries, follow new events with `sntry tail`
- **Multiple output formats** — text, JSON, NDJSON, ASCII table
- **Config-file auth** — credentials in a TOML file (default
  `~/.config/sntry/config.toml`, mode `0600`)
- **Multiple profiles** — switch between Sentry organizations / environments
- **Agent-friendly** — designed for AI agents (`-o ndjson -q`)
- **Self-hosted Sentry support** — set `host` per profile

## Install

### From source

```bash
cargo install --path .
```

### From crates.io

```bash
cargo install bcl-sntry
```

### Build manually

```bash
cargo build --release
cp target/release/sntry /usr/local/bin/
```

## Quick start

```bash
# 1. Create a User Auth Token at:
#    https://sentry.io/settings/account/api/auth-tokens/?name=sntry

# 2. Configure a profile.
sntry auth login

# 3. Verify.
sntry auth status
sntry orgs list

# 4. Investigate.
sntry issues list 'is:unresolved level:error' --from now-1h
sntry issues events ACME-API-1A2 --latest -o json
```

## Configuration

`sntry` reads a TOML file at:

| Path                                   | Notes                                        |
| -------------------------------------- | -------------------------------------------- |
| `$SNTRY_CONFIG`                        | Explicit override                            |
| `$XDG_CONFIG_HOME/sntry/config.toml`   | When `$XDG_CONFIG_HOME` is set               |
| `~/.config/sntry/config.toml`          | Default                                      |

Created with mode `0600` on first write; `sntry` refuses to read it if
the mode is loosened.

```toml
default_profile = "default"

[profiles.default]
host       = "sentry.io"
org        = "acme-co"
auth_token = "sntrys_..."
```

Per-field resolution order is **flag > env var > config file**:

| Field        | Env var               |
| ------------ | --------------------- |
| `auth_token` | `SENTRY_AUTH_TOKEN`   |
| `host`       | `SENTRY_HOST`         |
| `org`        | `SENTRY_ORG`          |

## Commands

```
sntry auth        # login / logout / use / list / status
sntry config      # path / show
sntry orgs        # list
sntry projects    # list / get
sntry issues      # list / get / events / update
sntry events      # get
sntry releases    # list / get
sntry discover    # query
sntry tail        # poll Discover for new events
```

`sntry --help` and `sntry <command> --help` are the canonical reference.

## Mutating state

`sntry issues update` is the only command that changes Sentry state. It is
gated behind an explicit `--yes`:

```bash
sntry issues update ACME-API-1A2 --status resolved --yes
```

## Agent usage

```bash
sntry issues list 'is:unresolved level:error' -f now-1h -o ndjson -q
```

- `-o ndjson` for streaming or large result sets
- `-o json` for a single JSON document
- `-q` to suppress progress / status output on stderr

## Specification

Full design notes and command reference: [`docs/specification.md`](docs/specification.md).

## License

MIT — see [LICENSE](LICENSE).
