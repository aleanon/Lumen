/* BENCH5 / GTK4 — see common.md for the contract every harness here obeys.
 *
 * Two traps this harness is written around, both of which caught earlier
 * rounds of this comparison:
 *
 * 1. `gtk_widget_measure` on a widget that is not rooted in a realized window
 *    returns instantly and measures nothing. BENCH3's first GTK harness
 *    reported sub-microsecond, perfectly flat timings for exactly that reason.
 *    The window here is shown and the main context pumped until the child
 *    actually has an allocation, and the harness aborts rather than print a
 *    number if it never gets one.
 *
 * 2. Paint. BENCH3 and BENCH4 both asserted that a synchronous snapshot
 *    "returns NULL", and left GTK with no paint row on that basis. That is
 *    WRONG, and this round measured it rather than inheriting it:
 *    `gtk_widget_paintable_new` + `gdk_paintable_snapshot` returns a perfectly
 *    good `GskRenderNode`. It is simply a STALE one. Serializing the node
 *    before and after changing a label's text gives byte-identical output
 *    (776404 bytes both times); only after the main loop is pumped does the
 *    serialization differ. GTK caches each widget's render node and rebuilds
 *    it during the frame clock's layout phase, not on demand.
 *
 *    So the conclusion survives but the reason changes: GTK's render-node
 *    build cannot be isolated synchronously. Worse, it cannot be isolated by
 *    timing the pump either — the frame clock is vsync-gated, so most pumps do
 *    no frame work at all and a minimum-of-N picks exactly those. This harness
 *    therefore measures GTK's paint the only way that is honest: SUSTAINED
 *    THROUGHPUT. It invalidates and runs the loop for a fixed wall-clock
 *    window, counting real frame-clock ticks. A result at ~16.7 ms/frame means
 *    "vsync-bound, the frame is cheaper than this and the instrument cannot
 *    resolve it"; anything slower is GTK's real end-to-end frame cost.
 *
 * Stages are CUMULATIVE so they sum to the total:
 *   set        gtk_label_set_text on the changed row(s)
 *   +measure   ... then gtk_widget_measure over the box (the relayout)
 * and, separately, the sustained end-to-end frame (see 2).
 */
#include <gtk/gtk.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <locale.h>

static int N = 3000, ITERS = 200, CHURN = 0;
static GtkWidget **rows;
static GtkWidget *win, *box, *sc;
static long rss_base = -1, rss_built = -1;
static int ticks = 0;

static gboolean tick_cb(GtkWidget *w, GdkFrameClock *c, gpointer u) {
    (void)w; (void)c; (void)u; ticks++; return G_SOURCE_CONTINUE;
}

static long proc_kb(const char *key) {
    FILE *f = fopen("/proc/self/status", "r");
    if (!f) return -1;
    char line[256]; long v = -1; size_t kl = strlen(key);
    while (fgets(line, sizeof line, f))
        if (!strncmp(line, key, kl)) { sscanf(line + kl, " %ld", &v); break; }
    fclose(f);
    return v;
}

static double now_us(void) {
    struct timespec ts; clock_gettime(CLOCK_MONOTONIC, &ts);
    return ts.tv_sec * 1e6 + ts.tv_nsec / 1e3;
}

static void pump(void) { while (g_main_context_pending(NULL)) g_main_context_iteration(NULL, FALSE); }

static void set_rows(int k) {
    char b[48];
    if (CHURN) {
        for (int i = 0; i < N; i++) {
            snprintf(b, sizeof b, "row %04d %05d", i, k);
            gtk_label_set_text(GTK_LABEL(rows[i]), b);
        }
    } else {
        snprintf(b, sizeof b, "row %04d %05d", 0, k);
        gtk_label_set_text(GTK_LABEL(rows[0]), b);
    }
}

static void measure_box(void) {
    int m, n2, a, b2;
    gtk_widget_measure(box, GTK_ORIENTATION_VERTICAL, 400, &m, &n2, &a, &b2);
    if (m <= 0) { fprintf(stderr, "gtk: measure returned %d - nothing measured\n", m); exit(2); }
}

static void on_activate(GtkApplication *app, gpointer _u) {
    (void)_u;
    setlocale(LC_ALL, "C");   /* gtk_init() re-reads it from the environment */
    rss_base = proc_kb("VmRSS:");
    win = gtk_application_window_new(app);
    gtk_window_set_default_size(GTK_WINDOW(win), 400, 800);
    box = gtk_box_new(GTK_ORIENTATION_VERTICAL, 0);
    rows = g_new0(GtkWidget *, N);
    for (int i = 0; i < N; i++) {
        char b[32]; snprintf(b, sizeof b, "row %d", i);
        rows[i] = gtk_label_new(b);
        gtk_box_append(GTK_BOX(box), rows[i]);
    }
    sc = gtk_scrolled_window_new();
    gtk_scrolled_window_set_child(GTK_SCROLLED_WINDOW(sc), box);
    gtk_window_set_child(GTK_WINDOW(win), sc);
    gtk_widget_set_visible(win, TRUE);
    /* A shown window is not a laid-out one; see trap 1 above. */
    for (int i = 0; i < 2000 && gtk_widget_get_width(sc) == 0; i++) { pump(); g_usleep(1000); }
    if (gtk_widget_get_width(sc) == 0) {
        fprintf(stderr, "gtk: widget never got an allocation\n");
        exit(2);
    }

    for (int k = 0; k < 20; k++) { set_rows(k); measure_box(); pump(); }
    rss_built = proc_kb("VmRSS:");

    printf("BENCH5\tfw=gtk4\tmode=%s\tn=%d\titers=%d\n", CHURN ? "churn" : "point", N, ITERS);

    double best_set = 1e18, best_layout = 1e18;
    for (int k = 0; k < ITERS; k++) {
        /* Untimed: leave the layout clean before each timed iteration, so
         * every iteration measures the same dirty->clean transition. */
        measure_box();
        double t0 = now_us();
        set_rows(k + 1000);
        double d = now_us() - t0;
        if (d < best_set) best_set = d;
    }
    for (int k = 0; k < ITERS; k++) {
        measure_box();
        double t0 = now_us();
        set_rows(k + 2000);
        measure_box();
        double d = now_us() - t0;
        if (d < best_layout) best_layout = d;
    }
    printf("stage.set\t%.1f\n", best_set);
    printf("stage.set_measure\t%.1f\n", best_layout);
    printf("total_us\t%.1f\n", best_layout);

    /* Trap 2: end-to-end frames through the real frame clock. A tick callback
     * fires once per frame GTK actually produces, so counting ticks over a
     * fixed window gives ms/frame including layout, render-node build, GSK
     * render and present — everything the synchronous route cannot reach. */
    {
        gtk_widget_add_tick_callback(win, tick_cb, NULL, NULL);
        ticks = 0;
        /* Warm: let the clock start and the first (expensive) frame pass. */
        double warm_end = now_us() + 500000.0;
        int seen = -1, k = 4000;
        while (now_us() < warm_end) {
            /* Mutate ONCE PER FRAME, not once per spin. The loop pumps far
             * faster than the clock ticks, so changing the text every spin
             * would charge the frame with thousands of `set_rows` calls and
             * report a throughput that has nothing to do with rendering. */
            if (ticks != seen) { seen = ticks; set_rows(k++); gtk_widget_queue_draw(win); }
            pump();
        }
        ticks = 0; seen = -1; k = 5000;
        double t0 = now_us(), end = t0 + 3000000.0;   /* 3 s window */
        while (now_us() < end) {
            if (ticks != seen) { seen = ticks; set_rows(k++); gtk_widget_queue_draw(win); }
            pump();
        }
        double elapsed_ms = (now_us() - t0) / 1000.0;
        if (ticks > 0) {
            double per = elapsed_ms / (double)ticks;
            printf("frame.sustained_ms\t%.2f\n", per);
            printf("frame.count\t%d\n", ticks);
            /* This display runs at 60 Hz, so 16.67 ms is the floor: at or
             * near it the measurement is vsync-bound and says only "the frame
             * is cheaper than this". The 10%% band above the floor absorbs the
             * occasional missed vsync that any windowed toolkit suffers. */
            printf("frame.vsync_floor_ms\t16.67\n");
            printf("frame.vsync_bound\t%s\n", per < 18.5 ? "yes" : "no");
        } else {
            printf("frame.sustained_ms\t-1\n");
            printf("frame.count\t0\n");
            printf("frame.vsync_bound\tunknown\n");
        }
    }

    printf("rss.base_kb\t%ld\n", rss_base);
    printf("rss.built_kb\t%ld\n", rss_built);
    printf("rss.peak_kb\t%ld\n", proc_kb("VmHWM:"));
    g_application_quit(G_APPLICATION(app));
}

int main(int argc, char **argv) {
    /* printf %f is locale-sensitive; a comma decimal breaks the runner's
     * parsing. GTK sets the locale from the environment during init, so this
     * has to be re-asserted rather than merely set once. */
    setlocale(LC_ALL, "C");
    if (argc > 1) N = atoi(argv[1]);
    if (argc > 2) ITERS = atoi(argv[2]);
    if (argc > 3 && !strcmp(argv[3], "churn")) CHURN = 1;
    GtkApplication *app = gtk_application_new("dev.lumen.bench5", G_APPLICATION_DEFAULT_FLAGS);
    g_signal_connect(app, "activate", G_CALLBACK(on_activate), NULL);
    int r = g_application_run(G_APPLICATION(app), 1, argv);
    g_object_unref(app);
    return r;
}
