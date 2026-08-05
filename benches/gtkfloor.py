import os, gi
gi.require_version('Gtk','3.0')
from gi.repository import Gtk, GLib
def rss():
    for l in open('/proc/self/status'):
        if l.startswith('VmRSS'): return l.split()[1]
print("after import gi+Gtk :", rss(), "kB")
w = Gtk.Window(title="floor"); w.set_default_size(900,600)
tv = Gtk.TreeView(); store = Gtk.TreeStore(bool,str,str,str,str,int,str,str,str,str,str,object)
for i in range(200):
    store.append(None,[False,f"pkg{i}",f"1.{i}",f"2.{i}","1 MB",1024,"x","y","z","a","b",None])
tv.set_model(store)
for c,t in enumerate(["Name","Old","New","Size"]):
    tv.append_column(Gtk.TreeViewColumn(t, Gtk.CellRendererText(), text=c+1))
sw = Gtk.ScrolledWindow(); sw.add(tv); w.add(sw); w.show_all()
print("after window+200-row TreeView:", rss(), "kB")
GLib.timeout_add(3000, lambda: (print("steady:", rss(), "kB"), Gtk.main_quit())[1])
Gtk.main()
