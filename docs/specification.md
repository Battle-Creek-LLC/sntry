# sntry CLI — Specification

A fast, minimal command-line interface for investigating Sentry issues, events,
and releases from the terminal — designed for humans **and** AI agents.

## Goals

- Query Sentry issues and events from the terminal with simple, memorable commands
- Cover the read-side surface that matters during an incident: list issues, drill
  into events, check release health, search via Discover
- Output results in human-readable and machine-parseable formats (JSON / NDJSON)
- Work well as a building block in shell pipelines and AI agent commands
- Switch between organizations / environments via named credential profiles

## Non-Goals

- Source map uploads, release artifact management, dSYM uploads — those remain
  the domain of the official `sentry-cli` from getsentry.com
- Project / team / member administration via the Sentry API
- Replacing the Sentry web UI for visual triage workflows
- Real-time event streaming (Sentry has no public streaming endpoint;
  `sntry tail` polls)

---

## Underlying API

Sentry exposes a REST API at `https://sentry.io/api/0/` (SaaS) or
`https://<self-hosted-host>/api/0/` (self-hosted). Authentication is via
`Authorization: Bearer <token>` using one of:

- **User Auth Token** — created at <https://sentry.io/settings/account/api/auth-tokens/>;
  scoped to the user, works across all orgs the user has access to.
- **Organization Auth Token** — created per org under
  `Settings → Auth Tokens`; scoped to a single org, recommended for CI and
  long-lived agents.

Both token types are accepted. The CLI does not care which is used as long as
the token has the scopes required for the requested operation
(`event:read`, `project:read`, `org:read`, etc.).

---

## Authentication & configuration

Credentials and per-profile config are stored in a TOML file managed via the
`sntry auth` and `sntry config` commands. Multiple **profiles** (credential
sets) are supported.

> Terminology note: a `sntry`-CLI **profile** is a credential set
> (e.g. `prod`, `staging`). It is not the same as a Sentry **project** (the
> resource inside an org that groups events). When the spec needs to refer to
> the Sentry resource explicitly, it will say "Sentry project" or use the
> `--sentry-project` flag.

### Config file location

| Path                                   | Notes                                                         |
| -------------------------------------- | ------------------------------------------------------------- |
| `$SNTRY_CONFIG` (if set)               | Explicit override                                             |
| `$XDG_CONFIG_HOME/sntry/config.toml`   | Used when `$XDG_CONFIG_HOME` is set                           |
| `~/.config/sntry/config.toml`          | Default fallback (XDG-compliant)                              |

The file is created with mode `0600` on first write. The CLI refuses to read
the file if its permissions are world- or group-readable and prints a hint to
re-run `chmod 600`. Tokens live in this file in plaintext, so file
permissions are the security boundary.

### Config file format

```toml
# Optional top-level state.
active_profile  = "prod"
default_output  = "text"   # text | json | ndjson | table

[profiles.default]
host       = "sentry.io"
org        = "acme-co"
auth_token = "sntrys_xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx"

[profiles.prod]
host       = "sentry.io"
org        = "acme-co"
auth_token = "sntrys_xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx"

[profiles.internal]
host       = "sentry.acme.co"
org        = "platform"
auth_token = "sntrys_xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx"
```

Recognized keys per profile:

| Key          | Required | Description                                          |
| ------------ | -------- | ---------------------------------------------------- |
| `host`       | yes      | Sentry host (`sentry.io` for SaaS)                   |
| `org`        | no       | Default org slug for commands that need one         |
| `auth_token` | yes      | User or Organization Auth Token                      |

Top-level keys:

| Key              | Description                                              |
| ---------------- | -------------------------------------------------------- |
| `active_profile` | Profile used when no `--profile` is passed (default: `default`) |
| `default_output` | Output format for non-TTY runs (default: `json`)         |

Unknown keys are preserved on rewrite (so users can keep their own annotations
or `# comments` — comments are preserved when feasible via `toml_edit`).

### `sntry auth login`

Interactive setup. Writes (or updates) a profile in `config.toml`.

```
sntry auth login [--profile <name>]
```

```
Profile name [default]: prod
Sentry host [sentry.io]:
Default organization slug: acme-co
Auth token: ********
Wrote profile 'prod' to ~/.config/sntry/config.toml
```

Options:

| Option              | Description                                              |
| ------------------- | -------------------------------------------------------- |
| `--profile <name>`  | Profile name (default: `default`)                        |
| `--host <HOST>`     | Sentry host (skip prompt)                                |
| `--org <SLUG>`      | Default org slug (skip prompt)                           |
| `--token <TOKEN>`   | Auth token (skip prompt). Use `--token -` to read stdin  |

When all options are provided, no interactive prompts are shown (scripted
setup). `--token -` lets agents pipe a token in without it landing in shell
history.

### `sntry auth logout [--profile <name>] [--all]`

Remove a profile (or all profiles) from `config.toml`. The file is rewritten
in place; if the resulting file would be empty, it is deleted.

### `sntry auth use <name>`

Set `active_profile` in `config.toml`.

### `sntry auth list`

List all configured profiles. Active profile is marked with `*`.

```
* prod      sentry.io          (org: acme-co)
  staging   sentry.io          (org: acme-staging)
  internal  sentry.acme.co     (org: platform)
```

### `sntry auth status`

Show the current authentication state (token is masked, e.g.
`sntrys_xxxx…dead`).

### `sntry config path`

Print the absolute path of the config file in use. Useful for `$EDITOR`
workflows: `$EDITOR "$(sntry config path)"`.

### `sntry config show [--profile <name>]`

Print the resolved config (token masked) as JSON or TOML.

### Profile Selection Order

1. `--profile <name>` flag
2. `SNTRY_PROFILE` env var
3. `active_profile` in `config.toml`
4. `default`

### Credential Lookup Order

For each resolved profile, individual fields are looked up in this order:

1. CLI flag (`--token`, `--host`, `--org`)
2. Environment variable (`SENTRY_AUTH_TOKEN`, `SENTRY_HOST`, `SENTRY_ORG`)
3. The matching key under `[profiles.<name>]` in `config.toml`

If env vars are set without any config file present, the CLI synthesizes an
ephemeral profile in memory — handy for CI and ephemeral agents that should
not write secrets to disk.

If no credentials can be resolved:
`Not authenticated. Run 'sntry auth login' to set up credentials.`

---

## Top-level UX

```
sntry [GLOBAL OPTIONS] <COMMAND> [ARGS]
```

### Global options

| Flag                      | Env var             | Default        | Notes                                              |
| ------------------------- | ------------------- | -------------- | -------------------------------------------------- |
| `--profile`, `-p`         | `SNTRY_PROFILE`     | (active)       | Use credentials from the named profile             |
| `--config`                | `SNTRY_CONFIG`      | (XDG default)  | Path to the TOML config file                       |
| `--org`, `-O`             | `SENTRY_ORG`        | (from profile) | Override the org slug for a single command         |
| `--sentry-project`, `-P`  | `SENTRY_PROJECT`    | —              | Restrict queries to a Sentry project slug          |
| `--output`, `-o`          | `SENTRY_OUTPUT`     | auto           | `text` \| `json` \| `ndjson` \| `table`            |
| `--color`                 | `NO_COLOR`          | auto           | `auto` \| `always` \| `never`                      |
| `--quiet`, `-q`           | —                   | false          | Suppress progress / status output on stderr        |
| `--verbose`, `-v`         | —                   | 0              | Repeatable; `-vv` debug, `-vvv` trace HTTP         |

### Output modes

- **`text`** — default when stdout is a TTY. Human-readable, colorized.
- **`json`** — single JSON document. Pretty-printed when TTY, compact otherwise.
  Default when stdout is *not* a TTY.
- **`ndjson`** — one JSON object per line. Preferred for streaming and large
  result sets.
- **`table`** — ASCII table of the most useful columns per command.

### Exit codes

| Code | Meaning                                                  |
| ---- | -------------------------------------------------------- |
| 0    | Success (including queries with zero results)            |
| 1    | Generic error (usage, parsing, query syntax)             |
| 2    | Auth error (401 / 403)                                   |
| 3    | Not found (404)                                          |
| 4    | Rate limited (429) after retries; `Retry-After` on stderr|
| 5    | Upstream 5xx after retries exhausted                     |
| 6    | Network / TLS error                                      |

---

## Commands — v0 surface

```
sntry auth        # login / logout / use / list / status (see above)
sntry config      # path / show — inspect the TOML config file
sntry orgs        # list / get
sntry projects    # list / get / stats
sntry issues      # list / get / events / update
sntry events      # get
sntry releases    # list / get
sntry discover    # query the Discover events endpoint (full search)
sntry tail        # poll Discover for new events matching a query
```

### `sntry orgs list`

Wraps `GET /api/0/organizations/`. Lists orgs accessible to the token.

### `sntry projects list [--org <slug>]`

Wraps `GET /api/0/organizations/{org}/projects/`. Lists Sentry projects in
the org. Includes platform, slug, ID, last-event timestamp.

### `sntry projects get <PROJECT_SLUG>`

Wraps `GET /api/0/projects/{org}/{project}/`. Returns full project detail.

### `sntry issues list`

Wraps `GET /api/0/organizations/{org}/issues/`. The investigation entrypoint.

```
sntry issues list [OPTIONS] [QUERY]
```

| Option              | Default     | Notes                                                                  |
| ------------------- | ----------- | ---------------------------------------------------------------------- |
| `QUERY`             | `is:unresolved` | Sentry issue search syntax                                         |
| `--from`, `-f`      | `now-24h`   | Start time (ISO 8601 or relative `now-1h`, `now-7d`)                   |
| `--to`, `-t`        | `now`       | End time                                                                |
| `--environment`     | —           | Filter by environment name                                              |
| `--project`, `-P`   | —           | Restrict to a Sentry project slug (repeatable)                          |
| `--sort`            | `date`      | `date` \| `new` \| `priority` \| `freq` \| `user`                       |
| `--limit`, `-n`     | `25`        | Page size (max 100 per Sentry API)                                      |
| `--max`             | `100`       | Stop paginating after N issues. `0` = unlimited                         |

**Example — human**

```
$ sntry issues list 'is:unresolved level:error' --from now-1h
SHORT_ID         LEVEL  COUNT  USERS  TITLE
ACME-API-1A2     error  1,204  187    DatabaseError: connection lost
ACME-API-1A3     error    412   53    TimeoutError: upstream took >30s
2 issues (842ms)
```

**Example — agent**

```
$ sntry issues list 'is:unresolved' -o ndjson --from now-1h
{"id":"...","shortId":"ACME-API-1A2","title":"DatabaseError: connection lost","count":1204,"userCount":187,"level":"error","lastSeen":"2026-05-09T18:04:12Z"}
```

### `sntry issues get <ISSUE_ID_OR_SHORT_ID>`

Wraps `GET /api/0/organizations/{org}/issues/{issue_id}/`. Returns the full
issue detail document including tags, latest event reference, status, assignee.

### `sntry issues events <ISSUE_ID_OR_SHORT_ID>`

Wraps `GET /api/0/organizations/{org}/issues/{issue_id}/events/`. Lists events
for an issue with the same time / pagination flags as `issues list`.

Special form: `--latest` returns just the most recent event (uses
`/issues/{id}/events/latest/`).

### `sntry issues update <ISSUE_ID_OR_SHORT_ID>`

Wraps `PUT /api/0/organizations/{org}/issues/{issue_id}/`.

| Option           | Notes                                                                |
| ---------------- | -------------------------------------------------------------------- |
| `--status`       | `resolved` \| `unresolved` \| `ignored`                              |
| `--assign-to`    | Username, email, or `team:<slug>`                                    |
| `--unassign`     | Clear assignee                                                       |

This is the only mutating command in v0. Hidden behind `--yes` for safety
unless `-q` is set explicitly to support agents that batch updates.

### `sntry events get <EVENT_ID>`

Wraps `GET /api/0/projects/{org}/{project}/events/{event_id}/`. Returns the
full event document — stack trace, breadcrumbs, tags, request, user, contexts.

If `--sentry-project` is not provided, attempts to resolve via the issue
membership endpoint first.

### `sntry releases list [--org <slug>]`

Wraps `GET /api/0/organizations/{org}/releases/`.

| Option            | Default | Notes                                          |
| ----------------- | ------- | ---------------------------------------------- |
| `--project`, `-P` | —       | Filter by Sentry project slug (repeatable)     |
| `--query`         | —       | Free-text release search                       |
| `--limit`, `-n`   | `25`    | Page size                                      |

### `sntry releases get <VERSION>`

Wraps `GET /api/0/organizations/{org}/releases/{version}/`. Returns release
detail including new groups, commit count, deploy timestamps.

### `sntry discover query`

Wraps `GET /api/0/organizations/{org}/events/` (the Discover query endpoint).
This is the most expressive search — equivalent to the Discover UI.

```
sntry discover query [OPTIONS] [QUERY]
```

| Option             | Default     | Notes                                                                 |
| ------------------ | ----------- | --------------------------------------------------------------------- |
| `QUERY`            | —           | Discover query string (e.g. `event.type:error transaction:/api/v1/*`) |
| `--field`          | `id,title,timestamp` | Repeatable. Fields to return (e.g. `--field count()`)        |
| `--from`, `-f`     | `now-24h`   | Start time                                                            |
| `--to`, `-t`       | `now`       | End time                                                              |
| `--sort`           | `-timestamp`| Sort spec (prefix `-` for descending)                                 |
| `--limit`, `-n`    | `100`       | Page size (max 1000)                                                  |
| `--max`            | `1000`      | Pagination ceiling                                                    |
| `--environment`    | —           | Filter by environment                                                 |
| `--dataset`        | `errors`    | `errors` \| `transactions` \| `discover` \| `issuePlatform`           |

**Example**

```
$ sntry discover query 'event.type:error level:error' \
    --field 'id,title,project,timestamp,user.id' \
    --from now-1h -o ndjson
```

### `sntry tail`

Stream new events matching a Discover query. Polls the Discover endpoint
(Sentry has no public WebSocket). Same flag set as `discover query`, plus:

| Option         | Default | Notes                                  |
| -------------- | ------- | -------------------------------------- |
| `--interval`   | `5s`    | Poll interval. Minimum 2s              |
| `--since`      | `now`   | Start timestamp for the first poll     |

Deduplicates across polls using event IDs. Exits on SIGINT with a summary line.

---

## Sentry Search Syntax (cheat sheet)

Issue / event queries use Sentry's search grammar. Quote in single quotes in
shell to protect colons and pipes from expansion.

```
# Issue status
is:unresolved
is:resolved
is:ignored

# Severity
level:error
level:warning

# Environment
environment:production

# User-scoped
user.id:42
user.username:alice

# Tags
release:my-app@1.4.2
browser.name:Chrome

# Free text (matches title and message)
"DatabaseError"

# Combinations (implicit AND)
is:unresolved level:error environment:production "Timeout"

# Negation
!environment:dev

# Time-bounded counts (Discover)
event.type:error has:user.id
```

Discover supports field aggregations:

```
# Count of errors per release
event.type:error                         # query
--field 'release,count()'                # SELECT
--field 'count()' --sort '-count()'      # ORDER BY
```

Recognized aggregation functions: `count`, `count_unique`, `count_if`, `min`,
`max`, `avg`, `sum`, `p50`, `p75`, `p95`, `p99`, `failure_rate`, `apdex`.

---

## Pagination

The Sentry API uses cursor-based pagination via the `Link` header
(`results="true"; cursor="<cursor>"`). The CLI auto-follows cursors until
`--max` is reached. Each cursor is opaque — the CLI never exposes it.

The `X-Hits` and `X-Max-Hits` headers are surfaced via `-v` for debugging.

---

## HTTP behavior

- Timeout: 30s per request (configurable via `SENTRY_TIMEOUT`).
- Retries: exponential backoff (1s, 2s, 4s) on 429 and 5xx, max 3 attempts,
  honors `Retry-After` if present.
- User-Agent: `sntry/<version>` (deliberately distinct from the official
  `sentry-cli` UA so it shows up cleanly in audit logs).
- TLS: native roots via `rustls-platform-verifier`.

---

## Output Formats

### `text` (default for TTY)

Compact, columnar. Truncated to fit terminal width. Common columns per command:

| Command            | Columns                                                |
| ------------------ | ------------------------------------------------------ |
| `issues list`      | `SHORT_ID  LEVEL  COUNT  USERS  TITLE`                 |
| `issues events`    | `EVENT_ID  TIMESTAMP  USER  RELEASE  MESSAGE`          |
| `projects list`    | `SLUG  PLATFORM  ID  LAST_EVENT`                       |
| `releases list`    | `VERSION  PROJECTS  NEW_ISSUES  DATE_CREATED`          |
| `orgs list`        | `SLUG  NAME  ROLE`                                     |

### `json` / `ndjson`

Raw API objects, no field renaming. ndjson preferred for paginated results.

### `table`

`comfy-table` ASCII output, all columns of the text mode plus extras.

### Empty results

- Exit code `0`
- `text`: prints `No results.` to stderr, nothing to stdout
- `json`: prints `[]`
- `ndjson`: prints nothing

---

## Error Handling

| Condition                  | Behavior                                                                                  |
| -------------------------- | ----------------------------------------------------------------------------------------- |
| Not authenticated          | Exit 2: `Not authenticated. Run 'sntry auth login' to set up credentials.`               |
| Profile not found          | Exit 1: `Profile '<name>' not found. Run 'sntry auth list' to see available profiles.`   |
| 401 / 403                  | Exit 2: `Authentication failed. Check token scopes with 'sntry auth status'.`            |
| 404                        | Exit 3 with the requested resource path                                                   |
| 429                        | Retry with backoff; if still 429, exit 4 with `Retry-After` hint                          |
| 5xx                        | Retry with backoff; exit 5 if all retries fail                                            |
| Bad query syntax (400)     | Exit 1 with the Sentry error message (e.g. `Invalid search query`)                        |
| Ctrl+C during `tail`       | Print summary line and exit 0                                                             |
| Config file unreadable     | Exit 1: `Unable to read <path>: <io error>`                                               |
| Config file world-readable | Exit 1: `Refusing to read <path>: file mode <mode>; run 'chmod 600 <path>'`               |
| Config file malformed      | Exit 1: `Invalid TOML in <path>: <parse error>`                                           |
| Network error              | Retry once after 2s, then exit 6                                                          |

---

## Agent Usage

Guidance for AI agents using the `sntry` CLI.

### Recommended flags

```bash
sntry issues list 'is:unresolved level:error' -f now-1h -o ndjson -q
```

- `-o ndjson` for streaming results (or `-o json` for a single doc)
- `-q` to suppress progress messages on stderr

### Investigation workflow

```bash
# 1. What's broken right now?
sntry issues list 'is:unresolved level:error' -f now-1h -n 10 -o json -q

# 2. Drill into the noisiest issue
sntry issues get ACME-API-1A2 -o json -q

# 3. Get the latest event payload (stack trace, breadcrumbs)
sntry issues events ACME-API-1A2 --latest -o json -q

# 4. Cross-reference with a release
sntry releases get my-app@1.4.2 -o json -q
```

### Mutations require explicit intent

`sntry issues update` only modifies state when `--yes` is passed (or
`-q` for agent contexts). Always show the user the prior state before
mutating.

---

## Architecture

Single-crate layout (mirrors `sumo`):

```
src/
  main.rs        # clap dispatch
  auth.rs        # Profile + token resolution
  config.rs      # TOML load / save / mode-check / migration
  http.rs        # reqwest client, retry/backoff, pagination
  output.rs      # text / json / ndjson / table writers
  time.rs        # Relative time parsing (now-1h, etc.)
  commands/
    auth.rs
    config.rs
    orgs.rs
    projects.rs
    issues.rs
    events.rs
    releases.rs
    discover.rs
    tail.rs
```

If the surface grows past `tail` we can split into a workspace
(`sentry-api`, `sentry-cli`, `sentry-config`) the way `ddog` did. Not needed
for v0.

### Dependencies

| Crate                         | Purpose                                  |
| ----------------------------- | ---------------------------------------- |
| `clap` (v4, derive)           | CLI parsing                              |
| `reqwest` (rustls)            | HTTP                                     |
| `tokio`                       | Async runtime (needed for `tail`)        |
| `serde` / `serde_json`        | (De)serialization                        |
| `chrono`                      | Timestamps + relative time               |
| `toml` / `toml_edit`          | Config parsing + comment-preserving rewrites |
| `directories`                 | XDG-compliant config path resolution     |
| `dialoguer`                   | Interactive `auth login` prompts         |
| `ctrlc`                       | SIGINT handling for `tail`               |
| `comfy-table`                 | Table output                             |
| `tracing` / `tracing-subscriber` | `-v` structured logs                  |
| `is-terminal`                 | TTY detection for output default         |
| `anyhow` / `thiserror`        | Error plumbing                           |

---

## Testing strategy

- Unit tests for time parsing, profile resolution, query escaping, and TOML
  load/save round-trips (including comment preservation and mode-0600 enforcement)
- Integration tests against a `wiremock` server covering: happy path, 401,
  404, 429 with retry, paginated cursor, ndjson streaming
- Snapshot tests (`insta`) for human text output
- No tests hit real Sentry in CI

---

## Build & Install

```bash
cargo build --release
cp target/release/sntry /usr/local/bin/
```

Or:

```bash
cargo install --path .
```

---

## Roadmap after v0

1. `sntry alerts {list,get,mute,unmute}` — alert rules + metric alerts
2. `sntry replays {list,get}` — Session Replay API
3. `sntry stats` — `/api/0/organizations/{org}/stats_v2/`
4. `sntry monitors {list,checkin}` — Cron monitor surface
5. `sntry teams {list,get}`
6. Shell completions (`sntry completions <shell>`)
7. Publish to crates.io + Homebrew tap (`jstockdi/homebrew-tap`)
8. Optional: SSO / OAuth flow for `auth login` instead of paste-the-token

---

## Resolved decisions

- **Credential storage: TOML file** at `~/.config/sntry/config.toml` (XDG
  fallback), with `[profiles.<name>]` sections and a top-level
  `active_profile`. Mode `0600` is enforced. No macOS Keychain dependency —
  works on Linux + macOS the same way, easier to bootstrap in CI.
- **Binary name: `sntry`.** Avoids collision with the official `sentry-cli`
  and is short to type.
- **Credential sets are called "profiles"** (not "projects") to avoid
  colliding with Sentry's own "project" resource.
- **`sntry issues update` ships in v0**, gated behind an explicit `--yes`
  flag. Triage from the terminal is a primary use case.
- **Default `--from` windows:** `now-24h` for `sntry issues list`, `now-1h`
  for `sntry discover query` and `sntry tail`.
- **Layout:** single-crate at v0; promote to a workspace if/when surface
  growth justifies it.

## Open questions for review

1. **Self-hosted Sentry smoke test.** `--host` already supports it, but a v0
   smoke test against a real self-hosted instance would be nice to have.
2. **Env var naming.** Spec uses `SENTRY_AUTH_TOKEN` / `SENTRY_HOST` /
   `SENTRY_ORG` to match conventions used by Sentry SDKs (so users with an
   existing env get it for free). Worth confirming we don't want a `SNTRY_*`
   namespace just for this CLI.
