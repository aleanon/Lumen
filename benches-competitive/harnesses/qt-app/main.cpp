// Minimal Qt6 windowed app — the size + idle-RSS counterpart of every other
// whole-app harness in this comparison. Same content as the others: a window
// with a label and a button, shown, then idle.
#include <QApplication>
#include <QWidget>
#include <QVBoxLayout>
#include <QLabel>
#include <QPushButton>

int main(int argc, char **argv) {
    QApplication app(argc, argv);
    QWidget w;
    auto *col = new QVBoxLayout(&w);
    auto *lbl = new QLabel("0");
    int count = 0;
    auto *btn = new QPushButton("Increment");
    QObject::connect(btn, &QPushButton::clicked, [&] { lbl->setText(QString::number(++count)); });
    col->addWidget(lbl);
    col->addWidget(btn);
    w.resize(400, 300);
    w.setWindowTitle("qt-app");
    w.show();
    return app.exec();
}
