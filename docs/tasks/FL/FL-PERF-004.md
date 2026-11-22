---
id: FL-PERF-004
title: A frame reaches the screen without GTK repainting the window (Wayland): the runner commits the window's surface itself
status: implemented
status_note: 2026-11-23; audited (audits/comprehensive-audit), findings fixed in AUD-REM-001; awaits review by another session (REVIEW_GUIDE)
depends_on: [FL-PERF-003]
order: 612
scope: apps/desktop-flutter/linux/runner/, docs/roadmap/verification/FL.md
---

# FL-PERF-004: A frame reaches the screen without GTK repainting the window

## Objective
After FL-PERF-003, the owner still found the app heavy compared with VS Code (2026-11-22).
Presenting a frame must not repaint the window in software, or keep GTK's thread (which
delivers input) or the compositor busy.

## Context
On Wayland, Flutter's GTK embedder (`FlViewRendererSubsurface`) presents each frame to a
*synchronized* subsurface of the window. A synchronized subsurface's buffer is applied only when
its parent commits, so after every frame the embedder calls `gtk_widget_queue_draw()`. GTK then:
- waits for its frame clock;
- repaints the whole window in software (4096×2304 at the owner's 125 %);
- commits that buffer, which KWin uploads again.

All of it happens on the thread that also delivers input, for every frame.

## Acceptance criteria
- `linux/runner/wayland_present.cc` defines `gtk_widget_queue_draw`, and the executable exports
  it (`ENABLE_EXPORTS`), so the embedder's call binds there. For Flutter's view it commits the
  window's `wl_surface` directly: no buffer, no damage. That applies the subsurface's waiting
  frame and nothing else. Every other widget, and X11, goes to GTK.
- `AXIOM_GTK_REDRAW=1` restores GTK's redraw, as an escape hatch and for comparison.
- The frame suite and the end-to-end suites pass.

## Result
`verification/FL.md` §Frame rate (the A/B). This depends on the embedder's internals:
- the type name `FlViewRendererSubsurface`;
- one `gtk_widget_queue_draw()` per frame.

If a Flutter upgrade changes either, the call reaches GTK unchanged and the app behaves as
before, slower. The upstream fix would be a desynchronized subsurface, or a commit without a
repaint, in the embedder itself.
