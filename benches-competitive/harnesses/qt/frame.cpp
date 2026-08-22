// Qt6 frame cost: a 3000-row list where one row's text changes each frame.
//
// The declarative frameworks in this comparison rebuild a view every frame;
// Qt does not — its widgets are retained, and a text change dirties one label.
// That asymmetry IS the comparison, so this measures what Qt actually has to
// do for the same user-visible change, in two separately reported halves:
//
//   layout   setText + invalidate + activate  — the relayout Lumen's build and
//                                               taffy pass are the analogue of
//   +paint   the same, then render() into a pixmap — adds rasterisation, which
//            the Rust harnesses do NOT do (they run a null renderer)
//
// Reporting them apart is the point: quoting only the paint number would
// flatter the Rust side, and quoting only layout would flatter Qt.
#include <QApplication>
#include <QLabel>
#include <QVBoxLayout>
#include <QWidget>
#include <QPixmap>
#include <QElapsedTimer>
#include <cstdio>
#include <clocale>
#include <vector>
#include <algorithm>

int main(int argc, char **argv) {
    const int N = argc > 1 ? atoi(argv[1]) : 3000;
    const int ITERS = argc > 2 ? atoi(argv[2]) : 200;
    qputenv("QT_QPA_PLATFORM", "offscreen");
    setlocale(LC_NUMERIC, "C");
    QApplication app(argc, argv);
    setlocale(LC_NUMERIC, "C"); // Qt resets it; printf must stay C-locale

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
    // Give the rows their NATURAL height, as the Rust harnesses do — a
    // QVBoxLayout asked to fit 3000 rows into an 800 px window squashes them to
    // nothing, which is a different (and much cheaper) layout problem than the
    // one every other framework here is solving.
    root.resize(400, root.sizeHint().height());
    root.show();
    app.processEvents();

    // Paint only the 400x800 viewport. Lumen culls off-screen nodes and iced
    // clips its draw to the viewport, so rendering all 3000 rows would be
    // measuring something no other row in the table measures.
    QPixmap pm(400, 800);
    const QRegion viewport(0, 0, 400, 800);

    auto bench = [&](const char *what, bool paint) {
        double best = 1e18;
        for (int k = 0; k < ITERS; ++k) {
            QElapsedTimer t;
            t.start();
            rows[0]->setText(QString("counter: %1").arg(k));
            root.layout()->invalidate();
            root.layout()->activate();
            if (paint) root.render(&pm, QPoint(), viewport);
            double us = t.nsecsElapsed() / 1000.0;
            best = std::min(best, us);
        }
        printf("qt/%-22s %9.1f us\n", what, best);
    };

    // Warm: first pass builds Qt's internal layout caches.
    for (int k = 0; k < 20; ++k) {
        rows[0]->setText(QString("warm %1").arg(k));
        root.layout()->invalidate();
        root.layout()->activate();
        root.render(&pm, QPoint(), viewport);
    }
    // Floor: no change at all, just the event-loop turn and the viewport
    // render. Every number below includes this, so it is reported rather than
    // buried — `processEvents()` is real work the Rust harnesses do not do.
    {
        double best = 1e18;
        for (int k = 0; k < ITERS; ++k) {
            QElapsedTimer t;
            t.start();
            app.processEvents();
            root.render(&pm, QPoint(), viewport);
            double us = t.nsecsElapsed() / 1000.0;
            best = std::min(best, us);
        }
        printf("qt/%-22s %9.1f us\n", "idle (floor)", best);
    }
    // The natural path: change the text and let Qt decide what to do. This is
    // what a real Qt app does, and it is the honest counterpart to Lumen's
    // patch tier — a retained toolkit does not rebuild or relayout when a
    // label's text changes without changing its size hint.
    {
        double best = 1e18;
        for (int k = 0; k < ITERS; ++k) {
            QElapsedTimer t;
            t.start();
            rows[0]->setText(QString("counter: %1").arg(k % 10000 + 10000));
            app.processEvents();
            root.render(&pm, QPoint(), viewport);
            double us = t.nsecsElapsed() / 1000.0;
            best = std::min(best, us);
        }
        printf("qt/%-22s %9.1f us\n", "natural update+paint", best);
    }
    {
        double best = 1e18;
        for (int k = 0; k < ITERS; ++k) {
            QElapsedTimer t;
            t.start();
            rows[0]->setText(QString("counter: %1").arg(k % 10000 + 10000));
            app.processEvents();
            double us = t.nsecsElapsed() / 1000.0;
            best = std::min(best, us);
        }
        printf("qt/%-22s %9.1f us\n", "natural update", best);
    }
    bench("forced relayout", false);
    bench("forced relayout+paint", true);
    return 0;
}
