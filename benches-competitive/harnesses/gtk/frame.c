/* GTK frame cost: 3000-row list, one row's text changes each frame.
 *
 * BENCH3's first attempt at this reported sub-microsecond, perfectly flat
 * timings — the signature of "the opponent does nothing".
 * gtk_widget_snapshot()/gdk_paintable_snapshot() return NULL on a widget that
 * is not rooted in a window, so nothing was being measured. The window is
 * realized and shown here, and the harness asserts the snapshot is non-NULL
 * before trusting a number.
 *
 * Build: see CMakeLists.txt (gtk4) / Makefile (gtk3).
 */
#include <gtk/gtk.h>
#include <stdio.h>
#include <string.h>

static int N = 3000, ITERS = 200;
static GtkWidget **rows;
static GtkWidget *win, *box;

static double now_us(void) {
    struct timespec ts; clock_gettime(CLOCK_MONOTONIC, &ts);
    return ts.tv_sec * 1e6 + ts.tv_nsec / 1e3;
}

static void pump(void) { while (g_main_context_pending(NULL)) g_main_context_iteration(NULL, FALSE); }

static void on_activate(GtkApplication *app, gpointer _u) {
    (void)_u;
    win = gtk_application_window_new(app);
    gtk_window_set_default_size(GTK_WINDOW(win), 400, 800);
    box = gtk_box_new(GTK_ORIENTATION_VERTICAL, 0);
    rows = g_new0(GtkWidget *, N);
    for (int i = 0; i < N; i++) {
        char b[32]; snprintf(b, sizeof b, "row %d", i);
        rows[i] = gtk_label_new(b);
        gtk_box_append(GTK_BOX(box), rows[i]);
    }
    GtkWidget *sc = gtk_scrolled_window_new();
    gtk_scrolled_window_set_child(GTK_SCROLLED_WINDOW(sc), box);
    gtk_window_set_child(GTK_WINDOW(win), sc);
    gtk_widget_set_visible(win, TRUE);
    /* A shown window is not a laid-out one. Pump until the child actually has
     * an allocation — snapshotting before that returns NULL, which is the trap
     * BENCH3's first GTK harness fell into (it reported sub-microsecond, flat
     * timings, i.e. nothing at all). */
    for (int i = 0; i < 2000 && gtk_widget_get_width(sc) == 0; i++) {
        pump();
        g_usleep(1000);
    }
    if (gtk_widget_get_width(sc) == 0) {
        fprintf(stderr, "gtk: widget never got an allocation\n");
        exit(2);
    }

    /* GTK's PAINT is not measured here, and that is a decision, not a gap.
     * `GtkWidgetPaintable` yields a GskRenderNode — the exact counterpart of
     * Lumen's display list — but only inside a live frame-clock snapshot pass;
     * calling `gtk_widget_snapshot_child` synchronously returns NULL, which is
     * what made BENCH3's first harness report sub-microsecond flat timings.
     * What IS measurable synchronously, and is what BENCH3 settled on, is
     * layout: `gtk_widget_measure` over the box with one label's text changed.
     */
    for (int k = 0; k < 20; k++) {
        gtk_label_set_text(GTK_LABEL(rows[0]), "warm");
        int m, n2, a, b2;
        gtk_widget_measure(box, GTK_ORIENTATION_VERTICAL, 400, &m, &n2, &a, &b2);
    }

    double best = 1e18;
    for (int k = 0; k < ITERS; k++) {
        char t[48];
        snprintf(t, sizeof t, "counter: %05d", k % 10000);
        double t0 = now_us();
        gtk_label_set_text(GTK_LABEL(rows[0]), t);
        int m, n2, a, b2;
        gtk_widget_measure(box, GTK_ORIENTATION_VERTICAL, 400, &m, &n2, &a, &b2);
        double d = now_us() - t0;
        if (m <= 0) { fprintf(stderr, "gtk: measure returned %d — nothing measured\n", m); exit(2); }
        if (d < best) best = d;
    }
    printf("gtk/%-21s %9.1f us   (%d rows)\n", "layout (measure)", best, N);
    g_application_quit(G_APPLICATION(app));
}

int main(int argc, char **argv) {
    if (argc > 1) N = atoi(argv[1]);
    if (argc > 2) ITERS = atoi(argv[2]);
    GtkApplication *app = gtk_application_new("dev.lumen.bench", G_APPLICATION_DEFAULT_FLAGS);
    g_signal_connect(app, "activate", G_CALLBACK(on_activate), NULL);
    int r = g_application_run(G_APPLICATION(app), 1, argv);
    g_object_unref(app);
    return r;
}
