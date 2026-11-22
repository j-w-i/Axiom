// FL-PERF-004: a Flutter frame reaches the screen without GTK repainting the window.
//
// On Wayland, Flutter's GTK embedder (FlViewRendererSubsurface) presents each frame to a
// synchronized subsurface of the window. A synchronized subsurface's new buffer is shown only
// when its parent surface commits, so after each frame the embedder calls
// gtk_widget_queue_draw() on its widget. GTK then waits for its own frame clock and repaints
// the whole window in software: at a fractional scale such as 125 %, a 4096x2304 buffer, on the
// thread that also delivers input, which the compositor must then upload again. That costs
// latency on every frame and makes the app feel heavy.
//
// The executable exports gtk_widget_queue_draw (ENABLE_EXPORTS), so the embedder's call binds
// here. For Flutter's view this commits the window's surface directly: no buffer, no damage,
// nothing repainted. The commit only applies the subsurface's waiting frame. Every other widget,
// and any case not on Wayland, goes to GTK unchanged. AXIOM_GTK_REDRAW=1 restores GTK's redraw
// (an escape hatch, and the comparison in docs/roadmap/verification/FL.md).

#include <dlfcn.h>
#include <gtk/gtk.h>

#ifdef GDK_WINDOWING_WAYLAND
#include <gdk/gdkwayland.h>
#include <wayland-client.h>
#endif

namespace {

using QueueDraw = void (*)(GtkWidget*);

QueueDraw real_queue_draw() {
  static QueueDraw real =
      reinterpret_cast<QueueDraw>(dlsym(RTLD_NEXT, "gtk_widget_queue_draw"));
  return real;
}

// Commits the surface of the window holding [widget]; false when that is not possible here.
bool commit_window_surface(GtkWidget* widget) {
#ifdef GDK_WINDOWING_WAYLAND
  GdkWindow* window = gtk_widget_get_window(gtk_widget_get_toplevel(widget));
  if (window == nullptr || !GDK_IS_WAYLAND_WINDOW(window)) {
    return false;
  }
  struct wl_surface* surface = gdk_wayland_window_get_wl_surface(window);
  if (surface == nullptr) {
    return false;
  }
  wl_surface_commit(surface);
  wl_display_flush(
      gdk_wayland_display_get_wl_display(gdk_window_get_display(window)));
  return true;
#else
  (void)widget;
  return false;
#endif
}

bool direct_commit_enabled() {
  static const bool enabled = g_getenv("AXIOM_GTK_REDRAW") == nullptr;
  return enabled;
}

bool is_flutter_view(GtkWidget* widget) {
  // Registered by the embedder when the view is created; looked up until then.
  static GType type = 0;
  if (type == 0) {
    type = g_type_from_name("FlViewRendererSubsurface");
  }
  return type != 0 && G_TYPE_CHECK_INSTANCE_TYPE(widget, type);
}

}  // namespace

extern "C" __attribute__((visibility("default"))) void gtk_widget_queue_draw(
    GtkWidget* widget) {
  if (widget != nullptr && direct_commit_enabled() && is_flutter_view(widget) &&
      commit_window_surface(widget)) {
    return;
  }
  QueueDraw real = real_queue_draw();
  if (real != nullptr) {
    real(widget);
  }
}
