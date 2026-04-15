# LTP Dashboard

A Rust-based dashboard for [Linux Test Project (LTP)][ltp]. Automatically
fetches, scores, and displays patches by priority in terminal or web UI.

[ltp]: https://github.com/linux-test-project/ltp

## Features

- **Intelligent Scoring**: Multi-factor priority scoring based on:
  - Patch version (higher versions = more refined)
  - Age (inverted-U curve: peaks at 31-60 days, decays for very
    old/stale patches)
  - Series size (standalone patches prioritized over large series)
  - State (`under-review` vs `new`)
  - Fix keywords (bug, regression, broken)
  - New test detection
  - Library/core changes
- **Priority Tiers**: P1 (≥80), P2 (≥60), P3 (≥40), P4 (≥20), P5 (<20)
- **Parallel Fetching**: Three-level parallelism for fast data retrieval
- **Review Detection**: Scans email replies for `Reviewed-by:` and
  `Acked-by:` tags
- **Two Output Modes**:
  - **Terminal**: Colored ANSI table output
  - **Web UI**: Dark-themed HTML interface with auto-refresh
- **Smart Caching**: 5-minute TTL with background refresh

## Installation

### Prerequisites

- Rust 1.70+

### Build

```bash
cargo build --release
```

## Usage

### Terminal Mode (default)

```bash
# Show top 100 patches in terminal
cargo run -- --max 100

# Include CI check results (slower)
cargo run -- --max 100 --checks
```

### Web Mode

```bash
# Start web server on port 3030
cargo run -- --web

# Custom port
cargo run -- --web --port 8080
```

Then open http://localhost:3030 in your browser.

## Configuration (optional)

Create `config.toml` in the project directory or
`~/.config/ltp-dashboard/config.toml`:

```toml
port = 3030
max_patches = 500
checks = false
```

**Priority**: `~/.config/ltp-dashboard/config.toml` > `./config.toml` >
defaults

## Command-Line Options

```
Options:
      --web         Expose a web interface instead of printing to
                    terminal
      --port <PORT> Port for the web server (only used with --web)
                    [default: 3030]
      --max <MAX>   Maximum number of patches to fetch [default: 500]
      --checks      Fetch CI check results for each patch (parallel)
  -h, --help        Print help
```

## Scoring System

### Priority Tiers

| Tier | Score | Color  | Meaning                               |
| ---- | ----- | ------ | ------------------------------------- |
| P1   | ≥80   | Red    | Critical: High-priority fixes/updates |
| P2   | ≥60   | Orange | Important: Should be reviewed soon    |
| P3   | ≥40   | Yellow | Moderate: Normal review queue         |
| P4   | ≥20   | Blue   | Low: Can wait                         |
| P5   | <20   | Gray   | Very Low: Stale, RFC, or low-impact   |

### Score Breakdown

- **Version**: v1=0, v2=15, v3=25, v4=35, v5+=40
- **Age (inverted-U curve)**:
  - <7 days: 0
  - 7-14 days: 10
  - 15-30 days: 20
  - **31-60 days: 45** (peak)
  - 61-90 days: 35
  - 91-180 days: 20
  - 181-365 days: 10
  - 366+ days: 5 (stale)
- **Series Size**: single=10, 2-5=0, 6-10=-5, 11+=-15
- **State**: `new`=5, `under-review`=0
- **Fix Keywords**: `fix`, `bug`, `regression`, `broken` = +10
- **New Test**: `new test`, `add test`, `rewrite` = +10
- **Library/Core**: `tst_`, `lib:`, `lapi`, `configure.ac` = +5
- **Penalties**:
  - RFC: -30
  - No activity (v1, <7 days): -20
  - Very large series (11+): -15

## Web UI Features

- **Dark Theme**: Easy on the eyes for long review sessions
- **Inline Badges**:
  - 🔴 **Stale**: >60 days old
  - 🟠 **RFC**: Request for comments
  - 🔵 **SERIES N**: Part of N-patch series
  - 🟣 **Under Review**: Currently being reviewed
  - 🟢 **Reviewed-by**: Has reviewer approval
  - 🔵 **Acked-by**: Has maintainer acknowledgment
- **Superseded**: Older version exists in the list
- **Green Highlight**: Patches with `Reviewed-by` or `Acked-by`
- **Auto-Refresh**: Updates every 5 minutes
- **Gzip Compression**: Faster page loads

### Data Flow

```
Patchwork API
     ↓
Parallel Fetch (states: new, under-review, needs-review-ack)
     ↓
Filter out cover letters
     ↓
Score each patch
     ↓
Sort by score (descending)
     ↓
Mark superseded patches
     ↓
Group by tier
     ↓
Output (Terminal or Web)
```

## Development

### Run Tests

```bash
cargo test
```

### Check Code

```bash
cargo clippy
cargo fmt --check
```

### Dependencies

- **axum**: Web framework
- **tokio**: Async runtime
- **reqwest**: HTTP client (rustls, HTTP/2)
- **serde/serde_json**: JSON serialization
- **minijinja**: HTML templating
- **chrono**: Date/time handling
- **regex**: Pattern matching
- **clap**: CLI parsing
- **anyhow**: Error handling
- **toml**: Config parsing

## License

This project is licensed under the GNU General Public License v3.0 or
later - see the [LICENSE](LICENSE) file for details.
