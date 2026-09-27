// Host-side operations QML cannot reach on its own.
//
// Kept deliberately small and enumerated: this is the boundary where the
// browser touches the desktop, so every addition here is a capability grant.
#pragma once

class QString;

namespace tela {

void set_clipboard_text(const QString &text);


} // namespace tela
