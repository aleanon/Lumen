import time, gi
gi.require_version('Gtk','3.0')
from gi.repository import Gtk, GLib

N = 500
w = Gtk.Window(title="rowbench"); w.set_default_size(400,400)
store = Gtk.ListStore(str)
for i in range(N): store.append([f"row {i} (static)"])
tv = Gtk.TreeView(model=store)
tv.append_column(Gtk.TreeViewColumn("Name", Gtk.CellRendererText(), text=0))
sw = Gtk.ScrolledWindow(); sw.add(tv); w.add(sw); w.show_all()

def pump():
    while Gtk.events_pending(): Gtk.main_iteration()

def bench():
    pump(); time.sleep(0.3); pump()
    it = store.get_iter_first()          # row 0, the one that changes
    ITERS = 2000
    # warm
    for k in range(50):
        store.set_value(it, 0, f"counter: {k}"); pump()
    t0 = time.perf_counter()
    for k in range(ITERS):
        store.set_value(it, 0, f"counter: {k}")
        pump()                            # includes the actual expose/draw
    t1 = time.perf_counter()
    per = (t1-t0)/ITERS*1e6
    print(f"GTK3 TreeView, {N} rows, 1 row changed + redraw: {per:.1f} us/update")
    # and a full-model rebuild for the other side of the comparison
    t0 = time.perf_counter()
    for k in range(200):
        store.clear()
        for i in range(N): store.append([f"row {i} (static)"])
        pump()
    t1 = time.perf_counter()
    print(f"GTK3 TreeView, {N} rows, FULL model rebuild + redraw: {(t1-t0)/200*1e6:.1f} us/update")
    Gtk.main_quit()

GLib.idle_add(bench)
Gtk.main()
