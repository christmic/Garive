/// Stable frontend event carrying one admitted native menu intent.
pub const DESKTOP_MENU_EVENT: &str = "desktop-menu";

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
}

impl DesktopMenuIntent {
    /// Returns the exact stable native menu identity.
    pub const fn id(self) -> &'static str {
        match self {
            Self::NewWork => "desktop.new-work",
            Self::Search => "desktop.search",
            Self::Settings => "desktop.settings",
            Self::ToggleInspector => "desktop.toggle-inspector",
        }
    }

    /// Parses only the closed safe native menu identity set.
    pub fn from_id(id: &str) -> Option<Self> {
        match id {
            "desktop.new-work" => Some(Self::NewWork),
            "desktop.search" => Some(Self::Search),
            "desktop.settings" => Some(Self::Settings),
            "desktop.toggle-inspector" => Some(Self::ToggleInspector),
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
        ];
        for intent in intents {
            assert_eq!(DesktopMenuIntent::from_id(intent.id()), Some(intent));
        }
        assert_eq!(DesktopMenuIntent::from_id("desktop.open-path"), None);
        assert_eq!(DesktopMenuIntent::from_id("/Users/private"), None);
    }
}
