#include "tela_cookies.h"

#include <QtCore/QDateTime>
#include <QtCore/QList>
#include <QtCore/QString>
#include <QtCore/QUrl>
#include <QtNetwork/QNetworkCookie>
#include <QtWebEngineCore/QWebEngineCookieStore>
#include <QtWebEngineQuick/QQuickWebEngineProfile>

#include "tela_profile.h"

namespace {

// What the store currently holds, kept in step with it by the two signals.
// Values are never read out of these — only domain, name and path, which are
// what deleteCookie needs to identify one.
QList<QNetworkCookie> g_cookies;
bool g_tracking = false;

QWebEngineCookieStore *store() {
    auto *profile = tela::profile();
    return profile ? profile->cookieStore() : nullptr;
}

// A cookie's domain as written covers the host it was set for and everything
// below it. A leading dot is the old way of saying the same thing.
QString bare(const QString &domain) {
    return domain.startsWith(QLatin1Char('.')) ? domain.mid(1) : domain;
}

// True when `domain` and `host` belong to the same site in either direction:
// the cookie's domain is the host, an ancestor of it (example.com covering
// www.example.com), or a descendant of it — "clear cookies for this site"
// means the site, not the exact label the user happens to be looking at.
//
// Matched on label boundaries, so example.com never covers notexample.com.
bool related(const QString &domain, const QString &host) {
    const QString a = bare(domain);
    if (a.isEmpty() || host.isEmpty()) {
        return false;
    }
    if (a.compare(host, Qt::CaseInsensitive) == 0) {
        return true;
    }
    return host.endsWith(QLatin1Char('.') + a, Qt::CaseInsensitive)
           || a.endsWith(QLatin1Char('.') + host, Qt::CaseInsensitive);
}

} // namespace

namespace tela {

void cookies_track() {
    if (g_tracking) {
        return;
    }
    auto *cookies = store();
    if (!cookies) {
        return;
    }
    g_tracking = true;

    QObject::connect(cookies, &QWebEngineCookieStore::cookieAdded,
                     [](const QNetworkCookie &cookie) { g_cookies.append(cookie); });
    QObject::connect(cookies, &QWebEngineCookieStore::cookieRemoved,
                     [](const QNetworkCookie &cookie) { g_cookies.removeAll(cookie); });

    // Replays cookieAdded for everything already stored, which is the only way
    // to see cookies written before this run.
    cookies->loadAllCookies();
}

int cookies_count_for_host(const QString &host) {
    int found = 0;
    for (const QNetworkCookie &cookie : std::as_const(g_cookies)) {
        if (related(cookie.domain(), host)) {
            ++found;
        }
    }
    return found;
}

int cookies_delete_for_host(const QString &host) {
    auto *cookies = store();
    if (!cookies) {
        return 0;
    }
    // Copied first: deleting emits cookieRemoved, which mutates g_cookies.
    QList<QNetworkCookie> doomed;
    for (const QNetworkCookie &cookie : std::as_const(g_cookies)) {
        if (related(cookie.domain(), host)) {
            doomed.append(cookie);
        }
    }
    for (const QNetworkCookie &cookie : std::as_const(doomed)) {
        cookies->deleteCookie(cookie);
    }
    return static_cast<int>(doomed.size());
}

int cookies_delete_all() {
    auto *cookies = store();
    if (!cookies) {
        return 0;
    }
    const int held = static_cast<int>(g_cookies.size());
    cookies->deleteAllCookies();
    return held;
}

void cookies_set(const QString &host, const QString &name, const QString &value,
                 const QString &path, bool secure, bool http_only,
                 int64_t expires_unix, int32_t same_site) {
    auto *cookies = store();
    if (!cookies) {
        return;
    }

    QNetworkCookie cookie;
    cookie.setName(name.toUtf8());
    cookie.setValue(value.toUtf8());
    cookie.setDomain(host);
    cookie.setPath(path);
    cookie.setSecure(secure);
    cookie.setHttpOnly(http_only);
    if (expires_unix > 0) {
        cookie.setExpirationDate(QDateTime::fromSecsSinceEpoch(expires_unix));
    }
    // Preserve SameSite. A Google auth cookie is SameSite=None so it can travel
    // to accounts.google.com when that origin is an *embedded* iframe (the
    // "Sign in with Google" / GSI flow). Dropping it to the browser default
    // makes it Lax, which is never sent cross-site, so GSI hangs on a blank
    // page. Firefox stores 0=None, 1=Lax, 2=Strict.
    switch (same_site) {
    case 0:  cookie.setSameSitePolicy(QNetworkCookie::SameSite::None);   break;
    case 2:  cookie.setSameSitePolicy(QNetworkCookie::SameSite::Strict); break;
    default: cookie.setSameSitePolicy(QNetworkCookie::SameSite::Lax);    break;
    }

    // setCookie needs an origin the cookie's domain and scheme agree with, or Qt
    // rejects it as a cross-origin write. A leading dot is a domain wildcard, not
    // part of the host.
    const QString bareHost = host.startsWith(QLatin1Char('.')) ? host.mid(1) : host;
    QUrl origin;
    origin.setScheme(secure ? QStringLiteral("https") : QStringLiteral("http"));
    origin.setHost(bareHost);
    origin.setPath(path.isEmpty() ? QStringLiteral("/") : path);

    cookies->setCookie(cookie, origin);
}

} // namespace tela
