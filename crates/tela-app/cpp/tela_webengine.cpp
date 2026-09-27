#include "tela_webengine.h"

#include <QtWebEngineQuick/QtWebEngineQuick>

namespace tela {

void init_web_engine() { QtWebEngineQuick::initialize(); }

} // namespace tela
