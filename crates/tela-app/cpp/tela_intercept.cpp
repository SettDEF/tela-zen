#include "tela_intercept.h"

#include <cstdio>
#include <QtCore/QByteArray>
#include <QtCore/QString>
#include <QtWebEngineCore/qtwebenginecoreglobal.h>
#include <QtWebEngineCore/QWebEngineProfile>

#include <QtWebEngineQuick/QQuickWebEngineProfile>

#include "tela_profile.h"
#include <QtWebEngineCore/QWebEngineUrlRequestInfo>
#include <QtWebEngineCore/QWebEngineUrlRequestInterceptor>

extern "C" bool tela_should_block(const char *url, const char *source_url,
                                  const char *request_type, const char *method);

namespace tela {
namespace {

// The version-bearing client hints, set from Rust before the interceptor is
// installed. Empty until then, in which case they are simply not stamped.
QByteArray g_ch_brands;
QByteArray g_ch_full_list;
QByteArray g_ch_full_version;

// The Firefox User-Agent to present on Google's sign-in host. Empty disables
// the quirk, leaving the normal Chrome identity.
QByteArray g_firefox_ua;

// Chromium's resource types mapped to the vocabulary filter lists use. Anything
// not listed falls through to "other", which is what the filter syntax expects
// for request kinds it has no specific keyword for.
const char *resource_type_name(QWebEngineUrlRequestInfo::ResourceType type) {
    switch (type) {
    case QWebEngineUrlRequestInfo::ResourceTypeMainFrame:    return "document";
    case QWebEngineUrlRequestInfo::ResourceTypeSubFrame:     return "sub_frame";
    case QWebEngineUrlRequestInfo::ResourceTypeStylesheet:   return "stylesheet";
    case QWebEngineUrlRequestInfo::ResourceTypeScript:       return "script";
    case QWebEngineUrlRequestInfo::ResourceTypeImage:        return "image";
    case QWebEngineUrlRequestInfo::ResourceTypeFontResource: return "font";
    case QWebEngineUrlRequestInfo::ResourceTypeMedia:        return "media";
    case QWebEngineUrlRequestInfo::ResourceTypeXhr:          return "xmlhttprequest";
    case QWebEngineUrlRequestInfo::ResourceTypePing:         return "ping";
    case QWebEngineUrlRequestInfo::ResourceTypeCspReport:    return "csp_report";
    case QWebEngineUrlRequestInfo::ResourceTypeObject:       return "object";
    case QWebEngineUrlRequestInfo::ResourceTypeWebSocket:    return "websocket";
    default:                                                 return "other";
    }
}

class BlockingInterceptor final : public QWebEngineUrlRequestInterceptor {
public:
    explicit BlockingInterceptor(QObject *parent = nullptr)
        : QWebEngineUrlRequestInterceptor(parent) {}

    void interceptRequest(QWebEngineUrlRequestInfo &info) override {
        // Runs on the IO thread for every single request, so this stays cheap:
        // no allocation beyond the URL conversions Qt already requires, and the
        // engine behind tela_should_block is read-only once built.
        const QByteArray url = info.requestUrl().toString().toUtf8();
        const QByteArray source = info.firstPartyUrl().toString().toUtf8();
        const QByteArray method = info.requestMethod();

        // Google's sign-in flow rejects Chromium-based embedded engines — the
        // credential-theft vector it is defending against is Chromium-specific,
        // so the block targets browsers that present as Chrome-in-a-webview.
        // Matching real Chrome exactly does not help: an embedded Chromium is
        // still detected. Presenting as Firefox side-steps the whole class of
        // check, and it is the fix qutebrowser — the same Qt WebEngine engine —
        // ships as a maintained default quirk. Scoped to the sign-in host, so
        // the rest of the web still sees Tela as the Chrome it is built on.
        const QString host = info.requestUrl().host();
        // The `/gsi/` endpoints are Google Identity Services — the embedded
        // "Sign in with Google" button and One-Tap that third-party sites use.
        // Those must NOT get the Firefox spoof: GSI compares the request's
        // User-Agent against the page's JavaScript UA (which stays Chrome), and
        // a Firefox header with a Chrome navigator hangs the flow on a blank
        // accounts.google.com/gsi/transform — the same UA-mismatch bug that
        // afflicts Waterfox but not real Firefox. Since a signed-in session
        // authenticates by cookie, GSI just needs a consistent Chrome identity.
        const QString path = info.requestUrl().path();
        const bool signInHost =
            (host == QLatin1String("accounts.google.com") ||
             host == QLatin1String("accounts.youtube.com")) &&
            !path.startsWith(QLatin1String("/gsi/"));

        if (signInHost && !g_firefox_ua.isEmpty()) {
            info.setHttpHeader("User-Agent", g_firefox_ua);

            // Real Firefox implements no User-Agent Client Hints, so it sends
            // none of the sec-ch-ua headers. Chromium sends them regardless of
            // the UA string, so a Firefox User-Agent arriving *with* sec-ch-ua
            // is a dead giveaway that it is Chromium in disguise — exactly the
            // thing the Firefox UA is meant to hide. Blank them so the request
            // matches a genuine Firefox. (Qt exposes no header removal; an empty
            // value is the closest, and is what a header-less Firefox is read
            // as here.)
            info.setHttpHeader("sec-ch-ua", QByteArray());
            info.setHttpHeader("sec-ch-ua-mobile", QByteArray());
            info.setHttpHeader("sec-ch-ua-platform", QByteArray());
            info.setHttpHeader("sec-ch-ua-full-version-list", QByteArray());
            info.setHttpHeader("sec-ch-ua-full-version", QByteArray());

            // Printed so it is visible whether this path fires on the real
            // domain — qWarning is swallowed in this build, so use stderr.
            static bool announced = false;
            if (!announced) {
                announced = true;
                fprintf(stderr,
                        "tela: presenting as Firefox on %s (Google sign-in quirk)\n",
                        host.toUtf8().constData());
            }
        } else if (!g_ch_brands.isEmpty()) {
            // Everywhere else, name a current Chrome — the client hints Chromium
            // builds itself say "Chromium" several majors stale, which some
            // sites gate on.
            info.setHttpHeader("sec-ch-ua", g_ch_brands);
            info.setHttpHeader("sec-ch-ua-full-version-list", g_ch_full_list);
            info.setHttpHeader("sec-ch-ua-full-version", g_ch_full_version);
        }

        // Never block the top-level navigation itself. A filter list matching a
        // document request would leave the user on a blank window with no way
        // to tell what happened.
        if (info.resourceType() == QWebEngineUrlRequestInfo::ResourceTypeMainFrame) {
            return;
        }

        if (tela_should_block(url.constData(), source.constData(),
                              resource_type_name(info.resourceType()),
                              method.constData())) {
            info.block(true);
        }
    }
};

} // namespace

void set_client_hints(const QString &brands, const QString &full_version_list,
                      const QString &full_version) {
    g_ch_brands = brands.toUtf8();
    g_ch_full_list = full_version_list.toUtf8();
    g_ch_full_version = full_version.toUtf8();
}

void set_signin_user_agent(const QString &firefox_ua) {
    g_firefox_ua = firefox_ua.toUtf8();
}

void install_request_interceptor() {
    if (auto *profile = tela::profile()) {
        // Parented to the profile, so its lifetime matches the thing it serves.
        // Parented to the profile, so its lifetime matches the thing it serves.
        profile->setUrlRequestInterceptor(new BlockingInterceptor(profile));
    }
}

} // namespace tela
