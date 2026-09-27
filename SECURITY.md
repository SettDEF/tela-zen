# Security model

A browser runs hostile code by design. Every page is untrusted input, and every
extension is a zip file from the internet with permission to read your tabs.
This file records the boundaries and, more usefully, the things this project
refuses to build.

## Things this project will not do

These are not "not yet". They are decisions.

- **No page-reachable command bridge.** Nothing a web page or content script can
  call may execute a shell command, spawn a process, or reach the filesystem
  directly. This is the single line that turns a browser into a persistence
  mechanism.
- **No `nativeMessaging`.** It is arbitrary process execution reachable from
  extension code. Extensions requesting it are marked unloadable and stay
  disabled — see `Severity::Blocking` in `manifest.rs`.
- **No `GM_xmlhttpRequest`.** Its entire purpose is issuing cross-origin
  requests that bypass CORS. Userscripts needing it fail on an undefined
  function rather than silently getting a capability they should not have.
- **No executable config.** Config files are read, never evaluated. A value in
  `config.json` can change a colour or a URL; it can never name something to run.

## Boundaries

### Extension bundles

An XPI or CRX is an untrusted archive. Two attacks matter, and both are covered
by tests in `crates/tela-extension/tests/security.rs`:

- **Zip slip.** Entries named `../../PWNED` must not escape the install
  directory. Extraction goes through `enclosed_name()`, which rejects absolute
  paths and `..` traversal. Tested against a real malicious archive with a
  canary file outside the target directory.
- **Absolute paths.** An entry named `/tmp/canary` must not be written there.
  Same defence, separate test.

### The `tela-extension://` scheme

Extension pages need a real origin, which means the handler is directly
reachable from web content.

- **Path traversal**: the resolved path is canonicalised and must still sit
  inside the bundle directory. A request for `../../.ssh/id_rsa` fails.
- **`web_accessible_resources` is enforced.** An extension may read its own
  files; anything else — a web page, another extension — gets only what the
  manifest explicitly published. Without this, any page could read an
  extension's entire bundle. Both Firefox and Chrome gate this, and so does the
  handler in `cpp/tela_scheme.cpp`.
- **No `LocalAccessAllowed`.** Extension pages have no business reaching
  `file://` URLs, and granting it would widen what a compromised extension can
  read.

### Script injection

- Userscripts and content scripts run in `WebEngineScript.UserWorld` — an
  isolated context sharing the page's DOM but not its globals. Page code cannot
  reach `GM_*`, and page code cannot clobber a script's variables.
- Every injected script is wrapped in its own try/catch, so one throwing script
  cannot take down the others sharing a page.
- Match patterns are evaluated in Rust before injection. A page matching nothing
  receives nothing — not an empty script, not the shim.

### Match patterns

Getting these subtly wrong is a security bug, not a cosmetic one: a pattern that
matches too broadly runs someone's script on a bank page. Tested cases include
`*.example.com` matching the bare domain but **not** `evil-example.com`, ports
and credentials ignored for host comparison, `*` scheme covering http/https but
not `file`/`ftp`, and excludes always beating includes.

## Known limitations

Stated plainly, because a security model with hidden gaps is worse than none:

- **`webRequest` is degraded, not absent.** Requests can be cancelled and
  redirected, but response bodies cannot be rewritten — `filterResponseData` is
  a Gecko-only API. Extensions relying on it for security filtering will be
  weaker here than on Firefox.
- **No extension signature verification.** Firefox checks AMO signatures; this
  loader does not yet. Installing an XPI is currently as trusting as running any
  downloaded archive.
- **No per-extension permission prompts.** Permissions are parsed and reported
  but not yet gated behind user consent at install time.

Both of the last two should land before the extension host is something to
recommend to anyone else.

## Reporting

This is a personal project with no release yet. If that changes, this section
gets a real contact and disclosure window.
