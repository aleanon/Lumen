//! Desktop system integration (T5.2): portable clipboard, native-menu model,
//! OS-service requests (notifications / file & color dialogs / tray), and
//! secondary-window descriptors.
//!
//! These are deterministic, agent-observable **data**: the headless layer holds
//! an in-memory clipboard, records system requests, and exposes the menu/window
//! model, so tests and `lumen-agent` drive them with no OS. The real binding
//! (winit windows, native menus, `rfd` dialogs, OS clipboard/notifications) is a
//! thin shell on top — it fulfils the same requests and feeds the same events
//! (drag-and-drop arrives as [`lumen_core::events::Event::Drop`]).

use serde::Serialize;

/// One node of a native menu (menu bar item or context-menu entry).
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct MenuItem {
    /// Stable command id (what `invoke_menu` matches).
    pub id: String,
    /// Display label.
    pub label: String,
    /// Whether the item is enabled.
    pub enabled: bool,
    /// Optional accelerator chord, e.g. `"Ctrl+O"` / `"Ctrl+Shift+S"`
    /// (P.3c). The shell fires the item when the chord is pressed — on
    /// Linux/winit this is the *only* native activation path (no menubar
    /// attachment point exists outside GTK); on Windows/macOS the native
    /// menu also registers it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub accel: Option<String>,
    /// Submenu items.
    pub children: Vec<MenuItem>,
}

impl MenuItem {
    /// A leaf command.
    pub fn new(id: impl Into<String>, label: impl Into<String>) -> MenuItem {
        MenuItem {
            id: id.into(),
            label: label.into(),
            enabled: true,
            accel: None,
            children: Vec::new(),
        }
    }

    /// A submenu.
    pub fn submenu(
        id: impl Into<String>,
        label: impl Into<String>,
        children: Vec<MenuItem>,
    ) -> MenuItem {
        MenuItem {
            id: id.into(),
            label: label.into(),
            enabled: true,
            accel: None,
            children,
        }
    }

    /// Attach an accelerator chord (e.g. `"Ctrl+O"`).
    pub fn accel(mut self, chord: impl Into<String>) -> MenuItem {
        self.accel = Some(chord.into());
        self
    }

    fn find<'a>(&'a self, id: &str) -> Option<&'a MenuItem> {
        if self.id == id {
            return Some(self);
        }
        self.children.iter().find_map(|c| c.find(id))
    }
}

/// A native menu tree (menu bar or context menu).
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct MenuModel {
    /// Top-level items.
    pub items: Vec<MenuItem>,
}

impl MenuModel {
    /// Find an item by command id, anywhere in the tree.
    pub fn find(&self, id: &str) -> Option<&MenuItem> {
        self.items.iter().find_map(|i| i.find(id))
    }
}

/// A request to an OS service. The headless layer records these; the real shell
/// fulfils them (and may feed a result back as an event).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SystemRequest {
    /// Show a desktop notification.
    Notification {
        /// Notification title.
        title: String,
        /// Notification body.
        body: String,
    },
    /// Open a native file-open dialog.
    OpenFile {
        /// Extension filters (e.g. `["png", "jpg"]`).
        filters: Vec<String>,
        /// Signal key the chosen path is written into when the shell
        /// fulfils the dialog (P.3b) — e.g. `"doc.path"`.
        reply: String,
    },
    /// Open a native file-save dialog.
    SaveFile {
        /// Suggested file name.
        suggested: String,
    },
    /// Set the system-tray tooltip.
    TrayTooltip(String),
}

/// A secondary window the app declares (multi-window).
#[derive(Clone, Debug, Serialize)]
pub struct WindowDesc {
    /// Stable window id (agent target).
    pub id: String,
    /// Window title.
    pub title: String,
    /// Logical width.
    pub width: f64,
    /// Logical height.
    pub height: f64,
}

/// Basic, dependency-free system information (E8.7). Memory/GPU details need a
/// platform `sysinfo` source (PENDING).
#[derive(Clone, Debug, Serialize)]
pub struct SystemInfo {
    /// Operating system (`linux`/`macos`/`windows`/…).
    pub os: String,
    /// CPU architecture (`x86_64`/`aarch64`/…).
    pub arch: String,
    /// Available parallelism (logical CPUs).
    pub cpus: usize,
}

/// Query basic system information from `std` (no external dependency).
pub fn system_info() -> SystemInfo {
    SystemInfo {
        os: std::env::consts::OS.to_string(),
        arch: std::env::consts::ARCH.to_string(),
        cpus: std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(1),
    }
}

/// Enqueue a [`SystemRequest`] from a handler (`Fn(&Runtime)`) — widgets
/// only hold `&Runtime`, so requests ride the runtime's host mailbox
/// (`Runtime::post`; transient, never snapshotted) and `pump` drains them
/// into `Headless::system_requests` (agent verb `app.systemRequests`; the
/// shell fulfils them).
pub fn queue_system(rt: &lumen_core::state::Runtime, req: SystemRequest) {
    rt.post(req);
}
