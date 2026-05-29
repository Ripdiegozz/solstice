# Tasks: UI Layout Redesign

## Review Workload Forecast

| Field | Value |
|-------|-------|
| Estimated changed lines | ~290 |
| 400-line budget risk | Low |
| Chained PRs recommended | No |
| Delivery strategy | single-pr-default |
| Chain strategy | pending |

Decision needed before apply: No
Chained PRs recommended: No
Chain strategy: pending
400-line budget risk: Low

## Phase 1: Foundation

- [ ] **T1. Add overlay state fields to `App`**  
  **Files**: `src/app.rs`  
  **Description**: Add `command_palette_open: bool` and `event_detail: Option<Event>` to `App` struct; initialize to `false` / `None` in `App::new`.  
  **Acceptance Criteria**: Compiles; no behavioral change.  
  **Estimated Lines**: 6

- [ ] **T2. Add command palette key handler**  
  **Files**: `src/app.rs`  
  **Description**: Implement `handle_command_palette_key(&mut self, key: KeyEvent) -> bool`. Esc / Ctrl+P / q close the palette; all other keys are absorbed.  
  **Acceptance Criteria**: Unit test: palette stays open on 'x', closes on Esc.  
  **Estimated Lines**: 15

- [ ] **T3. Add event detail key handler**  
  **Files**: `src/app.rs`  
  **Description**: Implement `handle_event_detail_key(&mut self, key: KeyEvent) -> bool`. Esc / Enter clear `event_detail`.  
  **Acceptance Criteria**: Unit test: detail closes on Esc/Enter.  
  **Estimated Lines**: 12

- [ ] **T4. Update mouse dismiss for new overlays**  
  **Files**: `src/app.rs`  
  **Description**: Extend `handle_mouse` to clear `command_palette_open` and `event_detail` on left-click when any overlay is active.  
  **Acceptance Criteria**: Unit test: both flags cleared after left-click.  
  **Estimated Lines**: 8

## Phase 2: Core Implementation

- [ ] **T5. Replace 3-line header with 1-line tabbed header**  
  **Files**: `src/ui/layout.rs`  
  **Description**: Change top constraint to `Length(1)`. Render tabs `[1]-Calendar [2]-Events [3]-Upcoming` left; compact clock right. Active tab uses accent color.  
  **Acceptance Criteria**: Header is 1 line; active tab highlighted; clock visible.  
  **Estimated Lines**: 30

- [ ] **T6. Replace dense footer with minimal hint**  
  **Files**: `src/ui/layout.rs`  
  **Description**: Replace full keybinding span bar with `ctrl+p: commands` hint on the footer right. Keep status message left.  
  **Acceptance Criteria**: Footer is 1 line; hint visible; no keybinding spans.  
  **Estimated Lines**: 20

- [ ] **T7. Add `render_command_palette`**  
  **Files**: `src/ui/event_form.rs`  
  **Description**: Centered 60%×40% overlay listing all commands. Use `Clear`, `Block` with accent border, base bg. Match `render_delete_confirm` signature pattern.  
  **Acceptance Criteria**: Buffer renders without panic; border and text visible.  
  **Estimated Lines**: 35

- [ ] **T8. Add `render_event_detail`**  
  **Files**: `src/ui/event_form.rs`  
  **Description**: Centered 50% width auto-height overlay showing event title, date, time, description, recurrence. Use `Clear`, `Block` with accent border, base bg.  
  **Acceptance Criteria**: Buffer renders without panic; all fields visible.  
  **Estimated Lines**: 40

## Phase 3: Integration

- [ ] **T9. Wire 5-way key dispatch in `main.rs`**  
  **Files**: `src/main.rs`  
  **Description**: Expand chain: modal → confirm_delete → command_palette_open → event_detail → normal. Add Ctrl+P toggle in normal mode; Enter clones selected event into `event_detail` for EventList/Upcoming.  
  **Acceptance Criteria**: All key scenarios from spec pass in manual test.  
  **Estimated Lines**: 30

- [ ] **T10. Render new overlays in `draw_ui`**  
  **Files**: `src/ui/layout.rs`  
  **Description**: After existing modal/confirm overlays, add palette and detail buffer-copy blocks following the existing `Buffer::empty` + cell-copy pattern.  
  **Acceptance Criteria**: Palette and detail render on top of base UI when flags are set.  
  **Estimated Lines**: 25

- [ ] **T11. Update mouse guard in `main.rs`**  
  **Files**: `src/main.rs`  
  **Description**: Extend overlay guard to include `command_palette_open` and `event_detail` so mouse hits dismiss them.  
  **Acceptance Criteria**: Mouse click dismisses palette and detail overlays.  
  **Estimated Lines**: 5

## Phase 4: Testing

- [ ] **T12. Unit tests for new App handlers**  
  **Files**: `src/app.rs` (tests)  
  **Description**: Test palette toggle/absorb, detail dismiss, normal-mode Enter opens detail, mouse dismisses overlays.  
  **Acceptance Criteria**: All new tests pass.  
  **Estimated Lines**: 45

- [ ] **T13. Integration tests for layout rendering**  
  **Files**: `src/ui/layout.rs` (tests)  
  **Description**: Assert 1-line header, minimal footer, and centered overlay dimensions.  
  **Acceptance Criteria**: Tests compile and pass.  
  **Estimated Lines**: 30
