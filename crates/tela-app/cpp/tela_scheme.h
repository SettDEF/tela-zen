// Serves tela-extension://<id>/<path> from unpacked extension bundles.
//
// Extension pages (popups, options, background) need a real origin: without
// one they cannot have their own localStorage, cannot be referenced by
// content scripts, and every relative URL inside the bundle breaks.
#pragma once

class QString;

namespace tela {

// Must run before the QGuiApplication exists — Chromium locks its scheme
// registry during initialisation.
void register_extension_scheme();

// Installs the handler on the default profile. Requires a live application.
void install_extension_handler(const QString &root);

// Override the default profile's User-Agent. Reaching the default profile from
// QML turned out to be unreliable (WebEngineProfile.defaultProfile resolves to
// undefined), and it has to be the *default* profile because that is where the
// extension scheme handler lives.
void set_user_agent(const QString &agent);

/// Chromium version behind this QtWebEngine, e.g. "140.0.7339.207".
QString chromium_version();

void set_accept_language(const QString &languages);

} // namespace tela
