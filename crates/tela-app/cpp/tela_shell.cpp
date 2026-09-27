#include "tela_shell.h"

#include <QtGui/QClipboard>
#include <QtGui/QGuiApplication>
#include <QtCore/QString>

namespace tela {

void set_clipboard_text(const QString &text) {
    if (auto *clipboard = QGuiApplication::clipboard()) {
        clipboard->setText(text);
    }
}

} // namespace tela
