/// Stable frontend event carrying one admitted native menu intent.
pub const DESKTOP_MENU_EVENT: &str = "desktop-menu";

/// Builds Garive's complete system-native application menu.
pub fn build_desktop_menu<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
) -> tauri::Result<tauri::menu::Menu<R>> {
    build_desktop_menu_for_locale(app, DesktopMenuLocale::English)
}

/// Builds the native application menu using one admitted live UI locale.
pub fn build_desktop_menu_for_locale<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    locale: DesktopMenuLocale,
) -> tauri::Result<tauri::menu::Menu<R>> {
    use tauri::menu::{Menu, MenuItemBuilder, SubmenuBuilder};
    let labels = menu_labels(locale);

    let new_work = MenuItemBuilder::with_id(DesktopMenuIntent::NewWork.id(), labels.new_work)
        .accelerator("CmdOrCtrl+N")
        .build(app)?;
    let search = MenuItemBuilder::with_id(DesktopMenuIntent::Search.id(), labels.search)
        .accelerator("CmdOrCtrl+F")
        .build(app)?;
    let settings = MenuItemBuilder::with_id(DesktopMenuIntent::Settings.id(), labels.settings)
        .accelerator("CmdOrCtrl+,")
        .build(app)?;
    let inspector =
        MenuItemBuilder::with_id(DesktopMenuIntent::ToggleInspector.id(), labels.inspector)
            .accelerator("CmdOrCtrl+Shift+A")
            .build(app)?;
    let zoom_in = MenuItemBuilder::with_id(DesktopMenuIntent::ZoomIn.id(), labels.zoom_in)
        .accelerator("CmdOrCtrl+=")
        .build(app)?;
    let zoom_out = MenuItemBuilder::with_id(DesktopMenuIntent::ZoomOut.id(), labels.zoom_out)
        .accelerator("CmdOrCtrl+-")
        .build(app)?;
    let actual_size =
        MenuItemBuilder::with_id(DesktopMenuIntent::ActualSize.id(), labels.actual_size)
            .accelerator("CmdOrCtrl+0")
            .build(app)?;

    let application = SubmenuBuilder::new(app, "Garive")
        .about_with_text(labels.about, None)
        .separator()
        .item(&settings)
        .separator()
        .services_with_text(labels.services)
        .separator()
        .hide_with_text(labels.hide)
        .hide_others_with_text(labels.hide_others)
        .show_all_with_text(labels.show_all)
        .separator()
        .quit_with_text(labels.quit)
        .build()?;
    let file = SubmenuBuilder::new(app, labels.file)
        .item(&new_work)
        .item(&search)
        .separator()
        .close_window_with_text(labels.close_window)
        .build()?;
    let edit = SubmenuBuilder::new(app, labels.edit)
        .undo_with_text(labels.undo)
        .redo_with_text(labels.redo)
        .separator()
        .cut_with_text(labels.cut)
        .copy_with_text(labels.copy)
        .paste_with_text(labels.paste)
        .select_all_with_text(labels.select_all)
        .build()?;
    let view = SubmenuBuilder::new(app, labels.view)
        .item(&zoom_in)
        .item(&zoom_out)
        .item(&actual_size)
        .separator()
        .item(&inspector)
        .separator()
        .fullscreen_with_text(labels.fullscreen)
        .build()?;
    let window = SubmenuBuilder::new(app, labels.window)
        .minimize_with_text(labels.minimize)
        .maximize_with_text(labels.maximize)
        .separator()
        .bring_all_to_front_with_text(labels.bring_all_to_front)
        .build()?;
    Menu::with_items(app, &[&application, &file, &edit, &view, &window])
}

/// Closed resolved locale identifiers accepted from the bundled frontend.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DesktopMenuLocale {
    /// English release locale.
    English,
    /// Simplified Chinese release locale.
    SimplifiedChinese,
    /// Expanded QA-only pseudolocale.
    Pseudolocale,
}

impl DesktopMenuLocale {
    /// Parses only resolved locales; the frontend must resolve `system` first.
    pub fn from_id(id: &str) -> Option<Self> {
        match id {
            "en" => Some(Self::English),
            "zh-Hans" => Some(Self::SimplifiedChinese),
            "en-XA" => Some(Self::Pseudolocale),
            _ => None,
        }
    }
}

struct MenuLabels {
    new_work: &'static str,
    search: &'static str,
    settings: &'static str,
    inspector: &'static str,
    zoom_in: &'static str,
    zoom_out: &'static str,
    actual_size: &'static str,
    about: &'static str,
    services: &'static str,
    hide: &'static str,
    hide_others: &'static str,
    show_all: &'static str,
    quit: &'static str,
    file: &'static str,
    close_window: &'static str,
    edit: &'static str,
    undo: &'static str,
    redo: &'static str,
    cut: &'static str,
    copy: &'static str,
    paste: &'static str,
    select_all: &'static str,
    view: &'static str,
    fullscreen: &'static str,
    window: &'static str,
    minimize: &'static str,
    maximize: &'static str,
    bring_all_to_front: &'static str,
}

fn menu_labels(locale: DesktopMenuLocale) -> MenuLabels {
    match locale {
        DesktopMenuLocale::English => MenuLabels {
            new_work: "New Work",
            search: "Search Work…",
            settings: "Settings…",
            inspector: "Toggle Inspector",
            zoom_in: "Zoom In",
            zoom_out: "Zoom Out",
            actual_size: "Actual Size",
            about: "About Garive",
            services: "Services",
            hide: "Hide Garive",
            hide_others: "Hide Others",
            show_all: "Show All",
            quit: "Quit Garive",
            file: "File",
            close_window: "Close Window",
            edit: "Edit",
            undo: "Undo",
            redo: "Redo",
            cut: "Cut",
            copy: "Copy",
            paste: "Paste",
            select_all: "Select All",
            view: "View",
            fullscreen: "Enter Full Screen",
            window: "Window",
            minimize: "Minimize",
            maximize: "Zoom",
            bring_all_to_front: "Bring All to Front",
        },
        DesktopMenuLocale::SimplifiedChinese => MenuLabels {
            new_work: "新建工作",
            search: "搜索工作…",
            settings: "设置…",
            inspector: "切换检查器",
            zoom_in: "放大",
            zoom_out: "缩小",
            actual_size: "实际大小",
            about: "关于 Garive",
            services: "服务",
            hide: "隐藏 Garive",
            hide_others: "隐藏其他",
            show_all: "全部显示",
            quit: "退出 Garive",
            file: "文件",
            close_window: "关闭窗口",
            edit: "编辑",
            undo: "撤销",
            redo: "重做",
            cut: "剪切",
            copy: "复制",
            paste: "粘贴",
            select_all: "全选",
            view: "显示",
            fullscreen: "进入全屏幕",
            window: "窗口",
            minimize: "最小化",
            maximize: "缩放",
            bring_all_to_front: "前置全部窗口",
        },
        DesktopMenuLocale::Pseudolocale => MenuLabels {
            new_work: "[Nëw Wôrk··]",
            search: "[Sëárch Wôrk…··]",
            settings: "[Sëttïngs…··]",
            inspector: "[Tôgglë Ïnspëctôr··]",
            zoom_in: "[Zôôm Ïn··]",
            zoom_out: "[Zôôm Ôüt··]",
            actual_size: "[Áctüál Sïzë··]",
            about: "[Ábôüt Gárïvë··]",
            services: "[Sërvïcës··]",
            hide: "[Hïdë Gárïvë··]",
            hide_others: "[Hïdë Ôthërs··]",
            show_all: "[Shôw Áll··]",
            quit: "[Qüït Gárïvë··]",
            file: "[Fïlë··]",
            close_window: "[Clôsë Wïndôw··]",
            edit: "[Ëdït··]",
            undo: "[Ündô··]",
            redo: "[Rëdô··]",
            cut: "[Cüt··]",
            copy: "[Côpy··]",
            paste: "[Pástë··]",
            select_all: "[Sëlëct Áll··]",
            view: "[Vïëw··]",
            fullscreen: "[Ëntër Füll Scrëën··]",
            window: "[Wïndôw··]",
            minimize: "[Mïnïmïzë··]",
            maximize: "[Zôôm··]",
            bring_all_to_front: "[Brïng Áll tô Frônt··]",
        },
    }
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

    #[test]
    fn native_menu_locale_is_closed_and_translates_product_and_standard_items() {
        assert_eq!(
            DesktopMenuLocale::from_id("en"),
            Some(DesktopMenuLocale::English)
        );
        assert_eq!(
            DesktopMenuLocale::from_id("zh-Hans"),
            Some(DesktopMenuLocale::SimplifiedChinese)
        );
        assert_eq!(
            DesktopMenuLocale::from_id("en-XA"),
            Some(DesktopMenuLocale::Pseudolocale)
        );
        assert_eq!(DesktopMenuLocale::from_id("system"), None);
        assert_eq!(DesktopMenuLocale::from_id("/Users/private"), None);

        let chinese = menu_labels(DesktopMenuLocale::SimplifiedChinese);
        assert_eq!(
            (chinese.file, chinese.new_work, chinese.zoom_in),
            ("文件", "新建工作", "放大")
        );
        assert_eq!(
            (chinese.about, chinese.quit, chinese.close_window),
            ("关于 Garive", "退出 Garive", "关闭窗口")
        );
        let pseudo = menu_labels(DesktopMenuLocale::Pseudolocale);
        assert!(pseudo.search.starts_with('[') && pseudo.search.ends_with("··]"));
    }
}
