//! MKW Track Editor: a Kirigami (Qt 6 / KF6) course editor for Mario Kart World, with
//! all logic in Rust (CXX-Qt).

mod config;
mod editor;
mod state;
mod thumbs;

use cxx_qt_lib::{QGuiApplication, QQmlApplicationEngine, QQuickStyle, QString, QUrl};

/// Dark colour scheme and Breeze icons shipped with the editor (Track Studio look). They are installed as a
/// default config layer (XDG_CONFIG_DIRS), so a user's own kdeglobals still wins, and
/// MKW_SYSTEM_COLORS=1 skips it.
fn install_color_scheme() {
    if std::env::var_os("MKW_SYSTEM_COLORS").is_some() {
        return;
    }
    let dir = config::cache_dir().join("xdg-config");
    // The app-specific file pins Breeze icons: KIconThemes reads it before the platform's
    // icon theme, which on GTK desktops lacks most of the KDE names the editor uses.
    let ok = std::fs::create_dir_all(&dir).is_ok()
        && std::fs::write(dir.join("kdeglobals"), include_str!("../data/kdeglobals")).is_ok()
        && std::fs::write(dir.join("mkw-track-editorrc"), "[Icons]\nTheme=breeze-dark\n").is_ok();
    if !ok {
        return;
    }
    let rest = std::env::var("XDG_CONFIG_DIRS").unwrap_or_else(|_| "/etc/xdg".into());
    // SAFETY: called first thing in main, before any other thread exists
    unsafe { std::env::set_var("XDG_CONFIG_DIRS", format!("{}:{rest}", dir.display())) };
}

fn main() {
    install_color_scheme();
    // KDE's desktop style unless the user picked another one
    if std::env::var_os("QT_QUICK_CONTROLS_STYLE").is_none() {
        QQuickStyle::set_style(&QString::from("org.kde.desktop"));
    }
    let mut app = QGuiApplication::new();
    if let Some(mut app) = app.as_mut() {
        app.as_mut().set_application_name(&QString::from("mkw-track-editor"));
        app.as_mut().set_application_display_name(&QString::from("MKW Track Editor"));
        app.as_mut().set_organization_domain(&QString::from("mkw.local"));
        app.as_mut().set_application_version(&QString::from(env!("CARGO_PKG_VERSION")));
    }
    QGuiApplication::set_desktop_file_name(&QString::from("org.mkw.trackeditor"));
    let mut engine = QQmlApplicationEngine::new();
    if let Some(engine) = engine.as_mut() {
        // MKW_QML_DIR loads Main.qml from disk instead of the built-in copy
        let url = match std::env::var("MKW_QML_DIR").ok().and_then(|d| std::path::Path::new(&d).canonicalize().ok()) {
            Some(dir) => format!("file://{}/Main.qml", dir.display()),
            None => "qrc:/qt/qml/org/mkw/trackeditor/qml/Main.qml".to_string(),
        };
        engine.load(&QUrl::from(url.as_str()));
    }
    if let Some(app) = app.as_mut() {
        app.exec();
    }
}
