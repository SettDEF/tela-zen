#include "tela_scheme.h"

#include <QtCore/QBuffer>
#include <QtCore/QDir>
#include <QtCore/QFile>
#include <QtCore/QFileInfo>
#include <QtCore/QJsonArray>
#include <QtCore/QJsonDocument>
#include <QtCore/QJsonObject>
#include <QtCore/QMimeDatabase>
#include <QtCore/QRegularExpression>
#include <QtCore/QString>
#include <QtWebEngineCore/QWebEngineProfile>

#include <QtWebEngineQuick/QQuickWebEngineProfile>

#include "tela_profile.h"
#include <QtWebEngineCore/qtwebenginecoreglobal.h>
#include <QtWebEngineCore/QWebEngineUrlRequestJob>
#include <QtWebEngineCore/QWebEngineUrlScheme>
#include <QtWebEngineCore/QWebEngineUrlSchemeHandler>

namespace tela {
namespace {

constexpr const char *kScheme = "tela-extension";

// Which files a page outside the extension may load.
//
// Without this every web page could read an extension's entire bundle — its
// source, its bundled data, anything on disk under the extension directory.
// Both Firefox and Chrome gate that behind web_accessible_resources, and so
// does this handler.
QStringList webAccessibleResources(const QString &bundleDir) {
    QFile manifest(QDir(bundleDir).filePath(QStringLiteral("manifest.json")));
    if (!manifest.open(QIODevice::ReadOnly)) {
        return {};
    }

    const QJsonDocument document = QJsonDocument::fromJson(manifest.readAll());
    const QJsonValue value = document.object().value(QStringLiteral("web_accessible_resources"));
    if (!value.isArray()) {
        return {};
    }

    QStringList patterns;
    for (const QJsonValue &entry : value.toArray()) {
        // MV2: a flat list of path globs.
        if (entry.isString()) {
            patterns << entry.toString();
            continue;
        }
        // MV3: objects carrying a "resources" list.
        if (entry.isObject()) {
            for (const QJsonValue &resource : entry.toObject().value(QStringLiteral("resources")).toArray()) {
                if (resource.isString()) {
                    patterns << resource.toString();
                }
            }
        }
    }
    return patterns;
}

bool matchesAnyPattern(const QStringList &patterns, const QString &path) {
    for (const QString &pattern : patterns) {
        const QRegularExpression expression(
            QRegularExpression::wildcardToRegularExpression(
                pattern, QRegularExpression::UnanchoredWildcardConversion));
        if (expression.match(path).hasMatch()
            && expression.match(path).capturedLength() == path.length()) {
            return true;
        }
    }
    return false;
}

class ExtensionSchemeHandler : public QWebEngineUrlSchemeHandler {
public:
    explicit ExtensionSchemeHandler(QString root, QObject *parent = nullptr)
        : QWebEngineUrlSchemeHandler(parent), m_root(std::move(root)) {}

    void requestStarted(QWebEngineUrlRequestJob *job) override {
        const QUrl url = job->requestUrl();
        const QString id = url.host();
        if (id.isEmpty()) {
            job->fail(QWebEngineUrlRequestJob::UrlInvalid);
            return;
        }

        QString path = url.path();
        if (path.startsWith('/')) {
            path.remove(0, 1);
        }
        if (path.isEmpty()) {
            path = QStringLiteral("index.html");
        }

        const QDir bundle(QDir(m_root).filePath(id));
        const QString canonicalRoot = bundle.canonicalPath();
        if (canonicalRoot.isEmpty()) {
            job->fail(QWebEngineUrlRequestJob::UrlNotFound);
            return;
        }

        // Resolve, then confirm the result is still inside the bundle. A
        // request for ../../.ssh/id_rsa must not escape the extension.
        const QString resolved = QFileInfo(bundle.filePath(path)).canonicalFilePath();
        if (resolved.isEmpty() || !resolved.startsWith(canonicalRoot + QDir::separator())) {
            job->fail(QWebEngineUrlRequestJob::UrlNotFound);
            return;
        }

        // An extension may read its own files freely. Anything else — a web
        // page, another extension — only gets what the manifest published.
        const QUrl initiator = job->initiator();
        const bool isSelf = initiator.scheme() == QLatin1String(kScheme)
                            && initiator.host() == id;
        // A top-level document load reports no initiator. That is how the
        // generated background page and an extension's own popup/options pages
        // arrive, and a web page cannot cause one — it can only issue
        // sub-resource requests, which do carry an initiator.
        const bool isTopLevel = initiator.isEmpty() || !initiator.isValid();
        if (!isSelf && !isTopLevel
            && !matchesAnyPattern(webAccessibleResources(bundle.path()), path)) {
            job->fail(QWebEngineUrlRequestJob::UrlNotFound);
            return;
        }

        // An extension's own HTML pages — dashboard, options, popup — need the
        // transport and browser.* shim in place before their own scripts run.
        // They are ordinary documents to Chromium, so nothing would otherwise
        // provide them, and every such page would load blank.
        //
        // Injected by reference rather than inlined: the three files are served
        // by this same handler, so they stay cached and the HTML stays readable
        // in devtools.
        if (resolved.endsWith(QLatin1String(".html"), Qt::CaseInsensitive)) {
            QFile source(resolved);
            if (source.open(QIODevice::ReadOnly)) {
                QByteArray html = source.readAll();
                static const QByteArray tags =
                    "<script src=\"/_tela/qwebchannel.js\"></script>"
                    "<script src=\"/_tela/prelude.js\"></script>"
                    "<script src=\"/_tela/shim.js\"></script>";

                // Before any of the page's own scripts. <head> if there is one,
                // otherwise the very start — a page without <head> still parses.
                const int head = html.indexOf("<head");
                if (head >= 0) {
                    const int close = html.indexOf('>', head);
                    if (close >= 0) {
                        html.insert(close + 1, tags);
                    }
                } else {
                    html.prepend(tags);
                }

                auto *buffer = new QBuffer(job);
                buffer->setData(html);
                buffer->open(QIODevice::ReadOnly);
                job->reply(QByteArrayLiteral("text/html"), buffer);
                return;
            }
        }

        auto *file = new QFile(resolved, job);
        if (!file->open(QIODevice::ReadOnly)) {
            job->fail(QWebEngineUrlRequestJob::UrlNotFound);
            return;
        }

        const QMimeDatabase mime;
        job->reply(mime.mimeTypeForFile(resolved).name().toUtf8(), file);
    }

private:
    QString m_root;
};

} // namespace

void register_extension_scheme() {
    QWebEngineUrlScheme scheme(kScheme);
    // Treated as secure and same-origin-capable so extension pages behave like
    // ordinary https origins; CORS is enabled so their fetches are governed by
    // the usual rules rather than being blanket-allowed.
    scheme.setSyntax(QWebEngineUrlScheme::Syntax::Host);
    scheme.setDefaultPort(QWebEngineUrlScheme::PortUnspecified);
    // Deliberately no LocalAccessAllowed: nothing here needs to reach file://
    // URLs, and granting it would widen what a compromised extension can read.
    // FetchApiAllowed matters in practice: extensions routinely fetch() their
    // own bundled JSON, and without it Chromium refuses the scheme outright.
    scheme.setFlags(QWebEngineUrlScheme::SecureScheme
                    | QWebEngineUrlScheme::CorsEnabled
                    | QWebEngineUrlScheme::FetchApiAllowed);
    QWebEngineUrlScheme::registerScheme(scheme);
}

void install_extension_handler(const QString &root) {
    auto *profile = tela::profile();
    if (!profile) {
        return;
    }
    profile->installUrlSchemeHandler(QByteArray(kScheme),
                                     new ExtensionSchemeHandler(root, profile));
}

void set_user_agent(const QString &agent) {
    if (agent.isEmpty()) {
        return;
    }
    if (auto *profile = tela::profile()) {
        profile->setHttpUserAgent(agent);
    }
}

// The Chromium version this Qt was built against. Claiming a Chrome version
// that does not match what navigator.userAgentData reports is the exact
// contradiction sites check for, so the version is read rather than hardcoded —
// a Qt upgrade must not silently turn the UA into a lie.
QString chromium_version() {
    return QString::fromLatin1(qWebEngineChromiumVersion());
}

// No Accept-Language at all is a stronger signal than an unusual one: every
// real browser sends it, and its absence marks a request as non-browser
// traffic regardless of the User-Agent.
void set_accept_language(const QString &languages) {
    if (languages.isEmpty()) {
        return;
    }
    if (auto *profile = tela::profile()) {
        profile->setHttpAcceptLanguage(languages);
    }
}

} // namespace tela
