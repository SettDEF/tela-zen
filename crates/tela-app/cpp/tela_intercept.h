#pragma once

class QString;

namespace tela {

/// Set the version-bearing client-hint headers the interceptor stamps on every
/// request, so they name a current Chrome rather than the engine's stale
/// Chromium. Computed in Rust from the installed browser; call before
/// install_request_interceptor.
void set_client_hints(const QString &brands, const QString &full_version_list,
                      const QString &full_version);

/// Set the Firefox User-Agent presented on Google's sign-in host, which rejects
/// Chromium-based embedded engines. Empty leaves the normal Chrome identity.
/// Call before install_request_interceptor.
void set_signin_user_agent(const QString &firefox_ua);

/// Put the filter engine on the request path for the default profile.
/// Requires a live application and must run before pages load.
void install_request_interceptor();

} // namespace tela
