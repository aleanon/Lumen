// BENCH5 / Qt6 — see common.md for the contract every harness here obeys.
//
// Qt is retained: a `setText` dirties one label, and the interesting question
// is what it costs Qt to get from that to a presentable frame. That is split
// into three CUMULATIVE stages so the rows sum to the total:
//
//   set      setText on the changed row(s). In `point` mode this is one call;
//            in `churn` mode it is N, and it is where Qt invalidates size
//            hints — the analogue of Lumen's build closure producing new text.
//   +layout  ... then invalidate() + activate(), i.e. the whole box layout.
//            This is the counterpart of Lumen's build+lower+taffy pass.
//   +paint   ... then render() into a 400x800 pixmap. NO other framework in
//            this comparison rasterises, so this row is reported for context
//            and must not be compared against a Rust total.
//
// The layout row is the like-for-like one, and even it is generous to Qt: it
// excludes the event-loop turn a real Qt app pays. `idle` reports that floor.
#include <QApplication>
#include <QLabel>
#include <QVBoxLayout>
#include <QWidget>
#include <QPixmap>
#include <QElapsedTimer>
#include <algorithm>
#include <clocale>
#include <cstdio>
#include <cstring>
#include <string>
#include <vector>

static long proc_kb(const char *key) {
    FILE *f = fopen("/proc/self/status", "r");
    if (!f) return -1;
    char line[256];
    long v = -1;
    while (fgets(line, sizeof line, f))
        if (!strncmp(line, key, strlen(key))) { sscanf(line + strlen(key), " %ld", &v); break; }
    fclose(f);
    return v;
}
#define RSS  proc_kb("VmRSS:")
#define HWM  proc_kb("VmHWM:")

int main(int argc, char **argv) {
    const int N = argc > 1 ? atoi(argv[1]) : 3000;
    const int ITERS = argc > 2 ? atoi(argv[2]) : 200;
    const bool churn = argc > 3 && !strcmp(argv[3], "churn");
    qputenv("QT_QPA_PLATFORM", "offscreen");
    setlocale(LC_NUMERIC, "C");
    QApplication app(argc, argv);
    setlocale(LC_NUMERIC, "C"); // Qt resets it; printf must stay C-locale

    const long rss_base = RSS;

    QWidget root;
    auto *col = new QVBoxLayout(&root);
    col->setSpacing(0);
    col->setContentsMargins(0, 0, 0, 0);
    std::vector<QLabel *> rows;
    rows.reserve(N);
    for (int i = 0; i < N; ++i) {
        auto *l = new QLabel(QString("row %1").arg(i));
        col->addWidget(l);
        rows.push_back(l);
    }
    // Natural height, as every other harness gives its rows. Squashing 3000
    // rows into an 800 px window is a different, much cheaper layout problem.
    root.resize(400, root.sizeHint().height());
    root.show();
    app.processEvents();

    QPixmap pm(400, 800);
    const QRegion viewport(0, 0, 400, 800);
    // Paint only the viewport: Lumen culls off-screen nodes, so rendering all
    // N rows would measure something no other row in the table measures.

    auto set = [&](int k) {
        if (churn) {
            for (int i = 0; i < N; ++i)
                rows[i]->setText(QString("row %1 %2").arg(i, 4, 10, QChar('0')).arg(k, 5, 10, QChar('0')));
        } else {
            rows[0]->setText(QString("row %1 %2").arg(0, 4, 10, QChar('0')).arg(k, 5, 10, QChar('0')));
        }
    };

    for (int k = 0; k < 20; ++k) { set(k); root.layout()->invalidate(); root.layout()->activate(); root.render(&pm, QPoint(), viewport); }
    const long rss_built = RSS;

    // stage: 0 = set only, 1 = set+layout, 2 = set+layout+paint.
    auto bench = [&](const char *what, int stage) {
        double best = 1e18;
        for (int k = 0; k < ITERS; ++k) {
            // Restore a CLEAN layout before each timed iteration, UNTIMED, so
            // every iteration measures the same dirty->clean transition.
            // Without this a `set`-only run leaves the layout permanently
            // invalid, and from the second iteration on `setText` skips the
            // size-hint invalidation it is meant to be measuring — the stage
            // row would then be cheap by construction rather than by merit.
            root.layout()->activate();
            QElapsedTimer t; t.start();
            set(k + 1000);
            if (stage >= 1) { root.layout()->invalidate(); root.layout()->activate(); }
            if (stage >= 2) root.render(&pm, QPoint(), viewport);
            best = std::min(best, t.nsecsElapsed() / 1000.0);
        }
        printf("stage.%s\t%.1f\n", what, best);
        return best;
    };
    // Floor: an event-loop turn plus a viewport render with nothing changed.
    {
        double best = 1e18;
        for (int k = 0; k < ITERS; ++k) {
            QElapsedTimer t; t.start();
            app.processEvents();
            root.render(&pm, QPoint(), viewport);
            best = std::min(best, t.nsecsElapsed() / 1000.0);
        }
        printf("stage.idle_paint\t%.1f\n", best);
    }
    printf("BENCH5\tfw=qt6\tmode=%s\tn=%d\titers=%d\n", churn ? "churn" : "point", N, ITERS);
    bench("set", 0);
    double total = bench("set_layout", 1);
    bench("set_layout_paint", 2);
    printf("total_us\t%.1f\n", total);
    printf("rss.base_kb\t%ld\n", rss_base);
    printf("rss.built_kb\t%ld\n", rss_built);
    printf("rss.peak_kb\t%ld\n", HWM);
    return 0;
}
