#include "drop.h"

#include <gtk/gtk.h>

#include <cstring>

namespace {

struct Drop {
  FlMethodChannel* channel;
  // What is held over the view has been asked for, and what it is comes by itself.
  gboolean asked;
  // It has been let go.
  gboolean dropped;
};

void drop_free(gpointer data, GClosure*) {
  Drop* drop = static_cast<Drop*>(data);
  g_object_unref(drop->channel);
  g_free(drop);
}

void tell(Drop* drop, const gchar* what, FlValue* args) {
  fl_method_channel_invoke_method(drop->channel, what, args, nullptr, nullptr,
                                  nullptr);
}

// What was sent, as paths, two at most: of more than one, only that there are
// more is of interest. What is on another computer is none.
GPtrArray* paths_of(GtkSelectionData* data) {
  GPtrArray* paths = g_ptr_array_new_with_free_func(g_free);
  g_auto(GStrv) uris = gtk_selection_data_get_uris(data);
  for (gchar** uri = uris; uri != nullptr && *uri != nullptr; uri++) {
    if (paths->len == 2) break;
    g_autofree gchar* host = nullptr;
    gchar* path = g_filename_from_uri(*uri, &host, nullptr);
    if (path == nullptr) continue;
    if (host != nullptr && *host != '\0' && strcmp(host, "localhost") != 0 &&
        strcmp(host, g_get_host_name()) != 0) {
      g_free(path);
      continue;
    }
    // Nor one that could not be told.
    if (!g_utf8_validate(path, -1, nullptr)) {
      g_free(path);
      continue;
    }
    g_ptr_array_add(paths, path);
  }
  return paths;
}

const gchar* at(GPtrArray* paths, guint i) {
  return static_cast<const gchar*>(g_ptr_array_index(paths, i));
}

gboolean motion_cb(GtkWidget* widget, GdkDragContext* context, gint, gint,
                   guint time, gpointer data) {
  Drop* drop = static_cast<Drop*>(data);
  GdkAtom target = gtk_drag_dest_find_target(widget, context, nullptr);
  if (target == GDK_NONE) return FALSE;
  // Never to be moved: whoever drags it would remove it.
  GdkDragAction may = gdk_drag_context_get_actions(context);
  gdk_drag_status(context,
                  (may & GDK_ACTION_COPY) != 0   ? GDK_ACTION_COPY
                  : (may & GDK_ACTION_LINK) != 0 ? GDK_ACTION_LINK
                                                 : static_cast<GdkDragAction>(0),
                  time);
  if (!drop->asked) {
    drop->asked = TRUE;
    gtk_drag_get_data(widget, context, target, time);
  }
  return TRUE;
}

void leave_cb(GtkWidget*, GdkDragContext*, guint, gpointer data) {
  Drop* drop = static_cast<Drop*>(data);
  drop->asked = FALSE;
  tell(drop, "out", nullptr);
}

gboolean drop_cb(GtkWidget* widget, GdkDragContext* context, gint, gint,
                 guint time, gpointer data) {
  Drop* drop = static_cast<Drop*>(data);
  GdkAtom target = gtk_drag_dest_find_target(widget, context, nullptr);
  if (target == GDK_NONE) return FALSE;
  drop->dropped = TRUE;
  gtk_drag_get_data(widget, context, target, time);
  return TRUE;
}

void received_cb(GtkWidget*, GdkDragContext* context, gint, gint,
                 GtkSelectionData* selection, guint, guint time,
                 gpointer data) {
  Drop* drop = static_cast<Drop*>(data);
  g_autoptr(GPtrArray) paths = paths_of(selection);
  if (drop->dropped) {
    drop->dropped = FALSE;
    g_autoptr(FlValue) args = fl_value_new_list();
    for (guint i = 0; i < paths->len; i++) {
      fl_value_append_take(args, fl_value_new_string(at(paths, i)));
    }
    gtk_drag_finish(context, paths->len > 0, FALSE, time);
    tell(drop, "drop", args);
  } else if (drop->asked && paths->len == 1 &&
             g_file_test(at(paths, 0), G_FILE_TEST_IS_DIR)) {
    g_autoptr(FlValue) args = fl_value_new_string(at(paths, 0));
    tell(drop, "over", args);
  }
}

}  // namespace

void drop_register(FlView* view) {
  Drop* drop = g_new0(Drop, 1);
  g_autoptr(FlStandardMethodCodec) codec = fl_standard_method_codec_new();
  FlBinaryMessenger* messenger =
      fl_engine_get_binary_messenger(fl_view_get_engine(view));
  drop->channel =
      fl_method_channel_new(messenger, "axiom/drop", FL_METHOD_CODEC(codec));

  GtkWidget* widget = GTK_WIDGET(view);
  gtk_drag_dest_set(widget, static_cast<GtkDestDefaults>(0), nullptr, 0,
                    static_cast<GdkDragAction>(GDK_ACTION_COPY | GDK_ACTION_LINK));
  gtk_drag_dest_add_uri_targets(widget);
  g_signal_connect(widget, "drag-motion", G_CALLBACK(motion_cb), drop);
  g_signal_connect(widget, "drag-leave", G_CALLBACK(leave_cb), drop);
  g_signal_connect(widget, "drag-drop", G_CALLBACK(drop_cb), drop);
  // Gone with the last of them.
  g_signal_connect_data(widget, "drag-data-received", G_CALLBACK(received_cb),
                        drop, drop_free, static_cast<GConnectFlags>(0));
}
