#include "tela_profile.h"

#include <QtCore/QObject>
#include <QtQml/QQmlApplicationEngine>
#include <QtWebEngineQuick/QQuickWebEngineProfile>

namespace {

QQuickWebEngineProfile *g_profile = nullptr;

} // namespace

namespace tela {

bool adopt_profile(QQmlApplicationEngine &engine) {
    if (g_profile) {
        return true;
    }

    for (QObject *root : engine.rootObjects()) {
        auto *found = root->findChild<QQuickWebEngineProfile *>(
            QStringLiteral("browserProfile"));
        if (found) {
            g_profile = found;
            return true;
        }
    }
    return false;
}

QQuickWebEngineProfile *profile() { return g_profile; }

} // namespace tela
