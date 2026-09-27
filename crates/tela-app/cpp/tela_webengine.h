// Qt WebEngine has one hard ordering requirement: QtWebEngineQuick::initialize()
// must run before the QGuiApplication is constructed. Rust's main() calls this
// first thing, before touching anything else in Qt.
#pragma once

namespace tela {

void init_web_engine();

} // namespace tela
