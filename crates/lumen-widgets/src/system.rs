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
#[derive(Clone, Debug, Serialize)]
pub struct MenuItem {
    /// Stable command id (what `invoke_menu` matches).
    pub id: String,
    /// Display label.
    pub label: String,
    /// Whether the item is enabled.
    pub enabled: bool,
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
            children,
        }
    }

    fn find<'a>(&'a self, id: &str) -> Option<&'a MenuItem> {
        if self.id == id {
            return Some(self);
        }
        self.children.iter().find_map(|c| c.find(id))
    }
}

/// A native menu tree (menu bar or context menu).
#[derive(Clone, Debug, Default, Serialize)]
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
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
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
