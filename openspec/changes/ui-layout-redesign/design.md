# Design: UI Layout Redesign

## Technical Approach

Replace the 3-line clock header with a 1-line tabbed header (tabs left, compact clock right). Replace the dense footer with a single `ctrl+p: commands` hint. Add two new overlay states—`command_palette_open` and `event_detail`—that follow the existing `confirm_delete` overlay pattern. Key dispatch in `main.rs` grows into a 5-way state machine (modal → confirm → palette → detail → normal). Overlays are rendered with the same `Buffer::empty(area)` + manual cell-copy technique already used for the modal and delete confirmation.

## Architecture Decisions

| Decision | Choice | Alternatives | Rationale |
|----------|--------|-------------|-----------|
| Overlay state exclusivity | Mutually exclusive flags (`modal`, `confirm_delete`, `command_palette_open`, `event_detail`) | Stacked overlays or enum state | Prevents z-order conflicts; matches existing code style |
| Overlay rendering | Buffer-based copy into `Frame` (existing pattern) | Custom `Widget` impl rendered via `f.render_widget` | Project already uses this for modal/confirm; avoids ratatui widget lifetime issues with `Frame` |
| Command palette location | Add functions to `src/ui/event_form.rs` | New module `src/ui/overlays.rs` | Minimal file churn; same function signature pattern (`Rect`, `&mut Buffer`, `&Config`) |
| Header clock | Compact 1-line time string directly in `layout.rs` | Modify `ClockWidget` to support compact mode | Keeps `ClockWidget` unchanged; simple `Span` line suffices for 1-line header |

## Data Flow

```
KeyEvent ──→ main.rs dispatch
  ├── modal.is_some()      ──→ app.handle_modal_key
  ├── confirm_delete       ──→ app.handle_delete_confirm
  ├── command_palette_open ──→ app.handle_command_palette_key
  ├── event_detail.is_some() ──→ app.handle_event_detail_key
  └── normal mode          ──→ numbered keys / app.handle_normal_key
                                   ├── Ctrl+P → toggle palette
                                   └── Enter  → clone event into event_detail
                                         (EventList/Upcoming only)
```

Mouse events: if any overlay flag is set, `handle_mouse` dismisses all overlays on left-click; otherwise hit-tests panels as before.

## File Changes

| File | Action | Description |
|------|--------|-------------|
| `src/app.rs` | Modify | Add `command_palette_open: bool` and `event_detail: Option<Event>`; add `handle_command_palette_key`, `handle_event_detail_key`; update `handle_mouse` to dismiss new overlays |
| `src/main.rs` | Modify | Expand key dispatch to palette/detail branches; update mouse guard |
| `src/ui/layout.rs` | Modify | 1-line header (tabs + compact clock); minimal footer (`ctrl+p: commands`); render palette and detail overlays after existing overlays |
| `src/ui/event_form.rs` | Modify | Add `render_command_palette` and `render_event_detail` following existing `render_modal`/`render_delete_confirm` signatures |

## Interfaces / Contracts

```rust
// src/app.rs — new fields and methods on App
pub command_palette_open: bool;
pub event_detail: Option<Event>;

pub fn handle_command_palette_key(&mut self, key: crossterm::event::KeyEvent) -> bool;
pub fn handle_event_detail_key(&mut self, key: crossterm::event::KeyEvent) -> bool;

// src/ui/event_form.rs — new render functions
pub fn render_command_palette(area: Rect, buf: &mut Buffer, config: &Config);
pub fn render_event_detail(event: &Event, area: Rect, buf: &mut Buffer, config: &Config);
```

**Key handling state machine (normal-mode bindings):**
- `Ctrl+P` — toggles `command_palette_open`
- `Enter` (EventList panel) — clones `selected_day_events[selected_event_index]` into `event_detail`
- `Enter` (Upcoming panel) — clones `upcoming_events[selected_upcoming_index]` into `event_detail`
- `Esc` / `Enter` (when `event_detail` open) — sets `event_detail = None`
- `Esc` / `Ctrl+P` / `q` (when palette open) — sets `command_palette_open = false`

All keys are absorbed when any overlay is active.

**Rendering order in `draw_ui`:**
1. Base UI (header → panels → footer)
2. If `modal` — `Clear` frame, render modal buffer, copy cells
3. If `confirm_delete` — render confirm buffer, copy cells
4. If `command_palette_open` — `Clear` frame, render palette buffer, copy cells
5. If `event_detail` — `Clear` frame, render detail buffer, copy cells

Mutual exclusion in `App` ensures only one overlay flag is ever true, so z-order is effectively undefined but safe.

## Testing Strategy

| Layer | What to Test | Approach |
|-------|-------------|----------|
| Unit | `handle_command_palette_key` toggles and absorbs keys | Direct method calls with synthetic `KeyEvent` |
| Unit | `handle_event_detail_key` closes on Esc/Enter | Direct method calls |
| Unit | `handle_normal_key` Enter opens detail only for EventList/Upcoming | Direct method calls with pre-populated events |
| Unit | Panel switching (`1`/`2`/`3`) is absorbed when palette/detail open | Assert main.rs dispatch routes to overlay handler, not normal mode |
| Unit | `handle_mouse` dismisses all overlays on left click | Set overlay flags, simulate click, assert all false |
| Integration | Header renders 1 line, footer renders hint | Terminal draw test or buffer assertion |

## Migration / Rollout

No migration required. Purely additive UI state; no config or data changes.

## Open Questions

None.
