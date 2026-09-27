// The browser profile: cookies, cache, user agent, and the hooks hung off it.
#pragma once

class QQmlApplicationEngine;
class QQuickWebEngineProfile;

namespace tela {

// Take up the profile declared in QML as `browserProfile`.
//
// It has to be declared in QML. A profile built in C++ — by `new`, or through
// QQmlComponent — reports itself persistent and keeps History and Favicons,
// but never gets a cookie store, with no error anywhere. Only a profile
// written into the loaded document does.
//
// Call after the engine has loaded and before the event loop starts: objects
// exist by then, but no URL has been fetched yet.
bool adopt_profile(QQmlApplicationEngine &engine);

// The one profile every web view, the extension scheme handler and the request
// interceptor share. Null until adopt_profile has run.
//
// It must be a single object: a handler installed on one profile does not
// apply to views using another, and the failure is silent — extensions stop
// resolving and requests stop being filtered.
QQuickWebEngineProfile *profile();

} // namespace tela
