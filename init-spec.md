Build a local-first TUI calendar application in Rust with the following requirements:

## Core Features
- Real-time clock display with configurable timezone
- Monthly/weekly calendar grid (using `ratatui` and `crossterm`)
- Public holidays display by country (hardcoded country code in config, loaded from a local JSON/TOML file bundled with the binary — use `https://docs.rs/holidays/latest/holidays/` or a curated static dataset)
- Upcoming events panel showing the next N events

## Data Storage
- All local events stored in a local SQLite database (`~/.local/share/solstice/events.db`) using `rusqlite`
- No remote server required — fully offline-capable

## Configuration
- TOML config file at `~/.config/solstice/config.toml`
- Configurable fields: timezone, country code (ISO 3166-1 alpha-2), theme colors, date format, first day of week

## Google Calendar Sync (optional, user-initiated)
- OAuth2 PKCE + Device Authorization Grant flow (no backend server needed)
- Store access/refresh token locally at `~/.config/solstice/gcal_token.json`
- Read-only sync: fetch events and cache them in the local SQLite DB
- Sync command: `solstice sync` (manual trigger, no background daemon)
- Use `oauth2` crate for the auth flow and `reqwest` for API calls

## TUI Layout
- Top bar: current time (large) + date + timezone label
- Center: calendar grid with highlighted today, holiday markers, and event dots
- Right panel: event list for selected day
- Bottom bar: keybindings hint
- Theme: support Catppuccin Mocha as default, customizable via omarchy themes config

## Tech Stack
- Rust edition 2026
- `ratatui` for TUI rendering
- `crossterm` as backend
- `chrono` + `chrono-tz` for date/time
- `rusqlite` for local storage
- `serde` + `toml` for config
- `oauth2` + `reqwest` for Google Calendar
- `tokio` async runtime (only for sync command, rest is sync)

## Project Structure

solstice/
├── src/
│ ├── main.rs
│ ├── app.rs # App state machine
│ ├── ui/ # ratatui widgets
│ ├── calendar/ # Grid logic, holiday loading
│ ├── events/ # SQLite CRUD
│ ├── gcal/ # OAuth2 + API sync
│ └── config.rs # TOML config loading
├── assets/
│ └── holidays/ # JSON files per country (e.g., CO.json, US.json)
└── Cargo.toml


Start by scaffolding the project structure, implementing the config loader, the clock widget, and the calendar grid with holiday markers. Leave Google Calendar sync as a stub for now.
