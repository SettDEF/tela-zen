use cxx_qt_build::{CxxQtBuilder, QmlModule};

fn main() {
    CxxQtBuilder::new_qml_module(
        QmlModule::new("Tela")
            .version(1, 0)
            .qml_files([
                // The window itself, and the tokens everything else reads from.
                "qml/Main.qml",
                "qml/Theme.qml",
                "qml/BookmarkSync.qml",
                // Browser furniture around the page.
                "qml/chrome/AddressBar.qml",
                "qml/chrome/FindBar.qml",
                "qml/chrome/RightBar.qml",
                "qml/chrome/Sidebar.qml",
                "qml/chrome/TabStrip.qml",
                // The page surfaces a tab can be showing.
                "qml/page/MediaPopout.qml",
                "qml/page/LiveBackground.qml",
                "qml/page/StartPage.qml",
                "qml/page/TabPage.qml",
                // Extension hosting: where their code runs and what they are told.
                "qml/extension/BackgroundHost.qml",
                "qml/extension/ExtensionActionPopup.qml",
                "qml/extension/ExtensionEvents.qml",
                "qml/extension/ExtensionRequests.qml",
                "qml/extension/ExtensionsPopup.qml",
                // Things that appear over the page.
                "qml/overlay/BackgroundPicker.qml",
                "qml/overlay/CommandPalette.qml",
                "qml/overlay/ContextMenu.qml",
                "qml/overlay/DownloadsPanel.qml",
                "qml/overlay/HistoryPanel.qml",
                "qml/overlay/Settings.qml",
                "qml/overlay/SiteInfoPopup.qml",
            ]),
    )
    // Network is here for QNetworkCookie, which the cookie store deals in.
    .qt_module("Network")
    .qt_module("Quick")
    .qt_module("QuickControls2")
    .qt_module("WebEngineQuick")
    .qt_module("WebEngineCore")
    .include_dir("cpp")
    .file("src/bookmarks.rs")
    .file("src/config.rs")
    .file("src/cookies.rs")
    .file("src/file_picker.rs")
    .file("src/firefox_import.rs")
    .file("src/history.rs")
    .file("src/extapi.rs")
    .file("src/extensions.rs")
    .file("src/session.rs")
    .file("src/shell.rs")
    .file("src/single_instance.rs")
    .file("src/webengine.rs")
    .cpp_file("cpp/tela_webengine.cpp")
    .cpp_file("cpp/tela_shell.cpp")
    .cpp_file("cpp/tela_profile.cpp")
    .cpp_file("cpp/tela_cookies.cpp")
    .cpp_file("cpp/tela_scheme.cpp")
    .cpp_file("cpp/tela_intercept.cpp")
    .build();
}
