# Proposal: UI Layout Redesign

## Intent

Replace the dense 3-line top bar and crowded footer with a clean tabbed header and minimal footer. Add a command palette (Ctrl+P) and a read-only event detail modal. This reduces visual clutter, surfaces panel navigation explicitly, and gives users a way to discover all commands without memorizing the footer.

## Scope

### In Scope
- Top-left tab labels with active highlight; clock moves to top-right
- Minimal footer replacing the full keybinding span bar
- Ctrl+P command palette overlay (toggle open/close)
- Enter key opens read-only event detail modal
- Two new lightweight overlay state fields mirroring `confirm_delete`

### Out of Scope
- New editing capabilities or form changes
- Mouse support for command palette
- Theme system changes beyond accent color usage

## Capabilities

### New Capabilities
- `command-palette`: Overlay listing all app commands; toggled via Ctrl+P; dismissed with Esc/q.
- `event-detail-modal`: Read-only modal showing selected event fields; triggered by Enter in normal mode.

### Modified Capabilities
- `ui-layout`: Top bar splits horizontally into tabs + clock. Footer shrinks to a single hint line.

## Approach

Split the 3-line top bar horizontally: left side renders tab labels `[1]-Calendar [2]-Events [3]-Upcoming`, right side renders `ClockWidget`. Active tab highlighted with accent color. No layout height changes.

Replace the dense footer with a minimal `ctrl+p: commands` hint.

Add `command_palette_open: bool` and `event_detail: Option<Event>` to `App` state, following the existing `confirm_delete` overlay pattern. Handle Ctrl+P and Enter in `main.rs` key dispatch. Render overlays in `layout.rs` after existing overlays.

## Affected Areas

| Area | Impact | Description |
|------|--------|-------------|
| `src/ui/layout.rs` | Modified | Split top bar; minimal footer; render new overlays |
| `src/app.rs` | Modified | Add `command_palette_open` and `event_detail` fields |
| `src/main.rs` | Modified | Handle Ctrl+P and Enter in key dispatch |

## Risks

| Risk | Likelihood | Mitigation |
|------|------------|------------|
| Overlay z-order conflicts with existing modal/confirm | Low | Render after existing overlays; state machine ensures mutual exclusion |
| Enter key conflicts with modal input | Low | Enter binding is active only in normal mode (no modal open) |

## Rollback Plan

Revert the three modified files. The change is purely additive state; no data migration or config changes required.

## Dependencies

None.

## Success Criteria

- [ ] Top bar shows 3 tabs with active highlight and clock on the right
- [ ] Footer shows only `ctrl+p: commands` hint
- [ ] Ctrl+P toggles a command palette overlay; Esc/q closes it
- [ ] Enter in normal mode opens a read-only event detail modal
- [ ] All existing panel switching, editing, and deletion flows remain intact
- [ ] Diff stays under 290 changed lines (well within 800-line review budget)
