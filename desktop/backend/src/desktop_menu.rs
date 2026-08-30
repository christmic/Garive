/// Stable frontend event carrying one admitted native menu intent.
pub const DESKTOP_MENU_EVENT: &str = "desktop-menu";

/// Builds Garive's complete system-native application menu.
pub fn build_desktop_menu<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
) -> tauri::Result<tauri::menu::Menu<R>> {
    use tauri::menu::{Menu, MenuItemBuilder, SubmenuBuilder};

    let new_work = MenuItemBuilder::with_id(DesktopMenuIntent::NewWork.id(), "New Work")
        .accelerator("CmdOrCtrl+N")
        .build(app)?;
    let search = MenuItemBuilder::with_id(DesktopMenuIntent::Search.id(), "Search Work…")
        .accelerator("CmdOrCtrl+F")
        .build(app)?;
    let settings = MenuItemBuilder::with_id(DesktopMenuIntent::Settings.id(), "Settings…")
        .accelerator("CmdOrCtrl+,")
        .build(app)?;
    let inspector =
        MenuItemBuilder::with_id(DesktopMenuIntent::ToggleInspector.id(), "Toggle Inspector")
            .accelerator("CmdOrCtrl+Shift+A")
            .build(app)?;
    let zoom_in = MenuItemBuilder::with_id(DesktopMenuIntent::ZoomIn.id(), "Zoom In")
        .accelerator("CmdOrCtrl+=")
        .build(app)?;
    let zoom_out = MenuItemBuilder::with_id(DesktopMenuIntent::ZoomOut.id(), "Zoom Out")
        .accelerator("CmdOrCtrl+-")
        .build(app)?;
    let actual_size = MenuItemBuilder::with_id(DesktopMenuIntent::ActualSize.id(), "Actual Size")
        .accelerator("CmdOrCtrl+0")
        .build(app)?;

    let application = SubmenuBuilder::new(app, "Garive")
        .about(None)
        .separator()
        .item(&settings)
        .separator()
        .services()
        .separator()
        .hide()
        .hide_others()
        .show_all()
        .separator()
        .quit()
        .build()?;
    let file = SubmenuBuilder::new(app, "File")
        .item(&new_work)
        .item(&search)
        .separator()
        .close_window()
        .build()?;
    let edit = SubmenuBuilder::new(app, "Edit")
        .undo()
        .redo()
        .separator()
        .cut()
        .copy()
        .paste()
        .select_all()
        .build()?;
    let view = SubmenuBuilder::new(app, "View")
        .item(&zoom_in)
        .item(&zoom_out)
        .item(&actual_size)
        .separator()
        .item(&inspector)
        .separator()
        .fullscreen()
        .build()?;
    let window = SubmenuBuilder::new(app, "Window")
        .minimize()
        .maximize()
        .separator()
        .bring_all_to_front()
        .build()?;
    Menu::with_items(app, &[&application, &file, &edit, &view, &window])
}

/// Safe action identities accepted from Garive's native application menu.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DesktopMenuIntent {
    /// Starts a fresh local Work composition.
    NewWork,
    /// Opens durable local Work search.
    Search,
    /// Opens Desktop settings.
    Settings,
    /// Shows or hides the current Work inspector.
    ToggleInspector,
    /// Increases the current WebView zoom by one bounded step.
    ZoomIn,
    /// Decreases the current WebView zoom by one bounded step.
    ZoomOut,
    /// Restores the current WebView to 100% zoom.
    ActualSize,
}

impl DesktopMenuIntent {
    /// Returns the exact stable native menu identity.
    pub const fn id(self) -> &'static str {
        match self {
            Self::NewWork => "desktop.new-work",
            Self::Search => "desktop.search",
            Self::Settings => "desktop.settings",
            Self::ToggleInspector => "desktop.toggle-inspector",
            Self::ZoomIn => "desktop.zoom-in",
            Self::ZoomOut => "desktop.zoom-out",
            Self::ActualSize => "desktop.actual-size",
        }
    }

    /// Parses only the closed safe native menu identity set.
    pub fn from_id(id: &str) -> Option<Self> {
        match id {
            "desktop.new-work" => Some(Self::NewWork),
            "desktop.search" => Some(Self::Search),
            "desktop.settings" => Some(Self::Settings),
            "desktop.toggle-inspector" => Some(Self::ToggleInspector),
            "desktop.zoom-in" => Some(Self::ZoomIn),
            "desktop.zoom-out" => Some(Self::ZoomOut),
            "desktop.actual-size" => Some(Self::ActualSize),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_menu_forwards_only_the_closed_safe_intent_set() {
        let intents = [
            DesktopMenuIntent::NewWork,
            DesktopMenuIntent::Search,
            DesktopMenuIntent::Settings,
            DesktopMenuIntent::ToggleInspector,
            DesktopMenuIntent::ZoomIn,
            DesktopMenuIntent::ZoomOut,
            DesktopMenuIntent::ActualSize,
        ];
        for intent in intents {
            assert_eq!(DesktopMenuIntent::from_id(intent.id()), Some(intent));
        }
        assert_eq!(DesktopMenuIntent::from_id("desktop.open-path"), None);
        assert_eq!(DesktopMenuIntent::from_id("/Users/private"), None);
    }
}
