#!/usr/bin/env bash
set -euo pipefail

CONFIG_DIR="${XDG_CONFIG_HOME:-$HOME/.config}/solstice"
CONFIG_FILE="$CONFIG_DIR/config.toml"

CONFIG_TEMPLATE='# Solstice configuration
# See https://github.com/yourusername/solstice for full documentation

# IANA timezone name
timezone = "America/Bogota"

# ISO 3166-1 alpha-2 country code (used for bundled holiday data)
country_code = "CO"

# Display formats (chrono specifiers)
date_format = "%A, %B %d, %Y"
time_format = "%H:%M:%S"

# First day of the week: "sunday" or "monday"
first_day_of_week = "sunday"

# Number of upcoming events shown in the sidebar
upcoming_event_count = 5

# Optional: Calendarific API key for online holiday fetching.
# Without this, Solstice uses bundled offline holiday data.
# Get a free key at https://calendarific.com
# calendarific_api_key = "your_key_here"

# Optional: Google Calendar OAuth2 credentials for two-way sync.
# gcal_client_id     = "your_client_id.apps.googleusercontent.com"
# gcal_client_secret = "your_client_secret"
'

print_step() { printf '\n\033[1;36m==> %s\033[0m\n' "$1"; }
print_ok()   { printf '\033[1;32m  ✓ %s\033[0m\n' "$1"; }
print_warn() { printf '\033[1;33m  ! %s\033[0m\n' "$1"; }
print_err()  { printf '\033[1;31m  ✗ %s\033[0m\n' "$1" >&2; }

# ── 1. Build & install ────────────────────────────────────────────────────────
print_step "Building and installing solstice"

if ! command -v cargo &>/dev/null; then
  print_err "cargo not found. Install Rust from https://rustup.rs"
  exit 1
fi

cargo install --path . --quiet
print_ok "Binary installed to $(which solstice 2>/dev/null || echo '~/.cargo/bin/solstice')"

# ── 2. Config setup ───────────────────────────────────────────────────────────
print_step "Checking configuration"

if [[ -f "$CONFIG_FILE" ]]; then
  print_warn "Config already exists at $CONFIG_FILE"
  printf '\n  Current contents:\n'
  sed 's/^/    /' "$CONFIG_FILE"
  printf '\n'

  printf '  What do you want to do?\n'
  printf '    [k] Keep existing config (default)\n'
  printf '    [o] Overwrite with fresh template\n'
  printf '    [q] Quit\n'
  printf '\n  Choice: '
  read -r choice

  case "${choice,,}" in
    o)
      cp "$CONFIG_FILE" "${CONFIG_FILE}.bak"
      print_ok "Backup saved to ${CONFIG_FILE}.bak"
      printf '%s' "$CONFIG_TEMPLATE" > "$CONFIG_FILE"
      print_ok "Config overwritten with template"
      ;;
    q)
      printf '\nAborted.\n'
      exit 0
      ;;
    *)
      print_ok "Keeping existing config"
      ;;
  esac
else
  mkdir -p "$CONFIG_DIR"
  printf '%s' "$CONFIG_TEMPLATE" > "$CONFIG_FILE"
  print_ok "Config created at $CONFIG_FILE"
fi

# ── 3. Done ───────────────────────────────────────────────────────────────────
printf '\n\033[1;32mDone! Run: solstice\033[0m\n\n'
