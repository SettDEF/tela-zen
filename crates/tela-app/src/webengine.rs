/// Bridge to the C++ shim that owns Qt WebEngine's process-wide setup.
#[cxx_qt::bridge(namespace = "tela")]
pub mod ffi {
    unsafe extern "C++" {
        include!("tela_webengine.h");

        /// Must be called before the QGuiApplication exists.
        fn init_web_engine();
    }

    unsafe extern "C++" {
        include!("cxx-qt-lib/qqmlapplicationengine.h");
        #[namespace = ""]
        type QQmlApplicationEngine = cxx_qt_lib::QQmlApplicationEngine;

        include!("tela_profile.h");

        /// Take up the profile declared in QML, after the engine has loaded
        /// and before the event loop starts.
        fn adopt_profile(engine: Pin<&mut QQmlApplicationEngine>) -> bool;
    }
}
