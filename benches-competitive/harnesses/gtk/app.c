/* Minimal GTK4 windowed app — size + idle-RSS counterpart. Same content as the
   Qt and Rust apps: a label and a button, shown, then idle. */
#include <gtk/gtk.h>
static int count = 0;
static GtkWidget *lbl;
static void on_click(GtkButton *b, gpointer u) { (void)b; (void)u;
    char t[32]; snprintf(t, sizeof t, "%d", ++count); gtk_label_set_text(GTK_LABEL(lbl), t); }
static void activate(GtkApplication *app, gpointer u) { (void)u;
    GtkWidget *w = gtk_application_window_new(app);
    gtk_window_set_default_size(GTK_WINDOW(w), 400, 300);
    gtk_window_set_title(GTK_WINDOW(w), "gtk-app");
    GtkWidget *box = gtk_box_new(GTK_ORIENTATION_VERTICAL, 8);
    lbl = gtk_label_new("0");
    GtkWidget *btn = gtk_button_new_with_label("Increment");
    g_signal_connect(btn, "clicked", G_CALLBACK(on_click), NULL);
#if GTK_MAJOR_VERSION >= 4
    gtk_box_append(GTK_BOX(box), lbl); gtk_box_append(GTK_BOX(box), btn);
    gtk_window_set_child(GTK_WINDOW(w), box);
    gtk_widget_set_visible(w, TRUE);
#else
    gtk_box_pack_start(GTK_BOX(box), lbl, FALSE, FALSE, 0);
    gtk_box_pack_start(GTK_BOX(box), btn, FALSE, FALSE, 0);
    gtk_container_add(GTK_CONTAINER(w), box);
    gtk_widget_show_all(w);
#endif
}
int main(int argc, char **argv) {
    GtkApplication *app = gtk_application_new("dev.lumen.gtkapp", G_APPLICATION_DEFAULT_FLAGS);
    g_signal_connect(app, "activate", G_CALLBACK(activate), NULL);
    int r = g_application_run(G_APPLICATION(app), argc, argv);
    g_object_unref(app); return r; }
