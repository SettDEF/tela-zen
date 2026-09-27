// Cookie removal, for the site-information panel.
//
// Removal only. Nothing here returns a cookie's value or writes one anywhere:
// the panel needs to answer "how many" and "delete them", and neither question
// requires reading what a cookie contains.
#pragma once

#include <cstdint>

class QString;

namespace tela {

// Start mirroring the profile's cookie store, so deletion has something to
// match against. Qt's store has no "list" call — the only way to know what
// exists is to observe cookieAdded, which loadAllCookies() replays for
// everything already on disk. Call once, after the profile is adopted.
void cookies_track();

// How many cookies are readable by `host`, counting the parent domains that
// set cookies for it and the subdomains it covers.
int cookies_count_for_host(const QString &host);

// Delete those cookies. Returns how many were asked to go.
int cookies_delete_for_host(const QString &host);

// Delete every cookie in the profile.
int cookies_delete_all();

// Write one cookie into the store, to bring a signed-in session over from
// another browser. `expires_unix` is seconds since the Unix epoch, 0 for a
// session cookie. The value is written straight through and never read back.
void cookies_set(const QString &host, const QString &name, const QString &value,
                 const QString &path, bool secure, bool http_only,
                 int64_t expires_unix, int32_t same_site);

} // namespace tela
