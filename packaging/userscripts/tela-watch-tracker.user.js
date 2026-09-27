// ==UserScript==
// @name         Tela Watch Tracker
// @namespace    tela.watch
// @version      2.0
// @description  Remembers where you stopped in any movie/series — even when the player is a cross-origin iframe — and offers to resume. Reads the series/season/episode from the page.
// @match        *://*/*
// @run-at       document-idle
// @grant        GM_setValue
// @grant        GM_getValue
// @grant        GM_addStyle
// @grant        GM_registerMenuCommand
// ==/UserScript==
(function () {
  "use strict";

  var isTop; try { isTop = window.top === window.self; } catch (e) { isTop = false; }

  // ── Media identity (host frame): what movie/series/episode is this? ─────
  function mediaInfo() {
    var info = { title: null, type: null, series: null, season: null, episode: null };
    var doc = document;
    try {
      var blocks = doc.querySelectorAll('script[type="application/ld+json"]');
      for (var i = 0; i < blocks.length; i++) {
        var data; try { data = JSON.parse(blocks[i].textContent); } catch (e) { continue; }
        var arr = Array.isArray(data) ? data : (data["@graph"] || [data]);
        for (var j = 0; j < arr.length; j++) {
          var d = arr[j]; if (!d || !d["@type"]) continue;
          var ty = String(d["@type"]).toLowerCase();
          if (ty.indexOf("movie") >= 0) { info.type = "movie"; info.title = info.title || d.name; }
          else if (ty.indexOf("episode") >= 0) {
            info.type = "episode"; info.episode = info.episode || d.episodeNumber; info.title = info.title || d.name;
            if (d.partOfSeason) info.season = info.season || d.partOfSeason.seasonNumber;
            if (d.partOfSeries) info.series = info.series || d.partOfSeries.name;
          } else if (ty.indexOf("tvseries") >= 0) { info.series = info.series || d.name; }
        }
      }
    } catch (e) {}
    function meta(p) { var m = doc.querySelector('meta[property="' + p + '"]') || doc.querySelector('meta[name="' + p + '"]'); return m && m.content ? m.content : null; }
    info.title = info.title || meta("og:title");
    var ogt = (meta("og:type") || "").toLowerCase();
    if (!info.type) { if (ogt.indexOf("movie") >= 0) info.type = "movie"; else if (ogt.indexOf("episode") >= 0) info.type = "episode"; }
    info.series = info.series || meta("video:series");
    info.season = info.season || meta("video:season");
    info.episode = info.episode || meta("video:episode");
    var se = seasonEpisode();
    if (se) { info.type = "episode"; info.season = info.season || se.s; info.episode = info.episode || se.e; if (!info.series && se.series) info.series = se.series; }
    if (!info.title) info.title = (doc.title || location.hostname).trim();
    info.title = cleanTitle(info.title);
    info.series = info.series ? cleanTitle(String(info.series)) : info.series;
    return info;
  }
  function titleize(slug) {
    if (!slug || /^\d+$/.test(slug)) return null;
    return slug.replace(/[-_+]+/g, " ").replace(/\s+/g, " ").trim().replace(/\b\w/g, function (c) { return c.toUpperCase(); });
  }
  // Season/episode + series name from the URL — the only reliable source on the
  // client-rendered TMDB SPAs (Cineby/P-Stream family) and the German portals.
  function seasonEpisode() {
    var path = location.pathname, q = location.search;
    var hay = (document.title || "") + " " + path + " " + q;
    // 1. Segment form: /tv|series|serie|watch|show/<id-or-slug>/<season>/<episode>
    //    Covers Cineby/P-Stream numeric (/tv/1396/1/1), slugged (/tv/1396-breaking-bad/2/5),
    //    and slug-only portals like bs.to (/serie/Breaking-Bad/2/5-EpisodeTitle).
    var m = path.match(/\/(?:tv|series|serie|watch|show|anime)\/([^/]+)\/(\d{1,3})\/(\d{1,3})(?:[-/]|$)/i);
    if (m) return { s: m[2], e: m[3], series: titleize(m[1].replace(/^\d+-?/, "")) };
    // 2. Query form: ?season=1&episode=5  /  ?s=1&e=5
    var qs = q.match(/[?&](?:season|s)=(\d{1,3})/i), qe = q.match(/[?&](?:episode|ep|e)=(\d{1,3})/i);
    if (qs && qe) return { s: qs[1], e: qe[1], series: null };
    // 3. German + textual: staffel-1/episode-5, staffel 1 folge 5, season-1/episode-5, temporada 1
    var t = hay.match(/(?:staffel|season|temporada|s(?:aison)?)[\s\-_.]*(\d{1,3})[\s\-_./]*(?:folge|episode|ep|cap(?:itulo)?|e)[\s\-_.]*(\d{1,3})/i);
    if (t) {
      var series = null;
      var sm = path.match(/\/(?:stream|serie|series|anime|watch)\/([a-z0-9][a-z0-9-]{1,})\/(?:staffel|season|saison|temporada)/i);
      if (sm) series = titleize(sm[1]);
      return { s: t[1], e: t[2], series: series };
    }
    // 4. Compact forms: S01E05, 1x05
    var x = hay.match(/\bS(\d{1,2})\s*[·:.\-\s]?\s*E(\d{1,2})\b/i) || hay.match(/\b(\d{1,2})x(\d{2})\b/);
    if (x) return { s: x[1], e: x[2], series: null };
    return null;
  }
  // Strip the site's own brand off the end of a title: "Spider-Man … | Cineby" → "Spider-Man …".
  // Brand comes from the hostname, so this works on any FMHY site without a per-site list.
  function cleanTitle(t) {
    if (!t) return t;
    t = String(t).trim();
    var parts = location.hostname.split(".");
    var brand = parts.length >= 2 ? parts[parts.length - 2] : parts[0]; // label before the TLD
    if (brand) {
      var re = new RegExp("\\s*[|\\-–—:»·▶]+\\s*" + brand.replace(/[.*+?^${}()|[\]\\]/g, "\\$&") + "[^|»\\-–—]*$", "i");
      t = t.replace(re, "").trim() || t;
    }
    // Generic streaming-boilerplate suffix ("… - Watch Online HD", "… kostenlos stream").
    t = t.replace(/\s*[|\-–—»·]\s*(watch\s*online(\s*hd)?|stream(ing)?\s*(hd)?|kostenlos.*|online\s*(anschauen|schauen).*|full\s*movie|hd\s*stream.*)\s*$/i, "").trim() || t;
    return t;
  }
  var _mi = null, _miK = null;
  function media() { var k = document.title + "|" + location.pathname; if (_mi && _miK === k) return _mi; _miK = k; _mi = mediaInfo(); return _mi; }
  function displayTitle(info) {
    if (info.type === "episode") {
      var base = info.series || info.title || "";
      var se = (info.season ? "S" + String(info.season).padStart(2, "0") : "") + (info.episode ? "E" + String(info.episode).padStart(2, "0") : "");
      return (base + (se ? " · " + se : "") + (info.title && info.title !== base ? " — " + info.title : "")).trim();
    }
    return (info.title || "").trim();
  }
  function itemKey() {
    var info = media(); var base = location.href.split("#")[0].split("?")[0];
    if (info.type === "episode" && (info.season || info.episode)) return base + "#s" + (info.season || 0) + "e" + (info.episode || 0);
    return base;
  }
  function fmt(s) { s = Math.floor(s); var h = Math.floor(s / 3600), m = Math.floor((s % 3600) / 60), c = s % 60; return (h ? h + ":" + (m < 10 ? "0" : "") : "") + m + ":" + (c < 10 ? "0" : "") + c; }

  // ── Storage (host frame only) ───────────────────────────────────────────
  var STORE = "tela.watch.v1";
  function loadAll() { try { return JSON.parse(GM_getValue(STORE, "{}")) || {}; } catch (e) { return {}; } }
  function saveAll(o) { GM_setValue(STORE, JSON.stringify(o)); }
  function recordProgress(time, duration) {
    if (!duration || time < 2) return;
    var all = loadAll();
    if (duration - time < 90 && time / duration > 0.9) { delete all[itemKey()]; saveAll(all); return; }
    var info = media();
    all[itemKey()] = {
      time: time, duration: duration, title: displayTitle(info) || location.href, type: info.type || "video",
      series: info.series || null, season: info.season || null, episode: info.episode || null,
      url: location.href.split("#")[0].split("?")[0], host: location.hostname, ts: Date.now(),
    };
    var keys = Object.keys(all); if (keys.length > 300) { keys.sort(function (a, b) { return all[a].ts - all[b].ts; }); delete all[keys[0]]; }
    saveAll(all);
  }

  // ── Player role: any frame that has the <video>. Reports to the top. ────
  function pickVideo() {
    var vids = Array.prototype.slice.call(document.querySelectorAll("video"));
    var playing = vids.filter(function (v) { return !v.paused && v.currentTime > 0 && v.duration > 0; });
    var pool = playing.length ? playing : vids.filter(function (v) { return v.duration > 0; });
    pool.sort(function (a, b) { return b.clientWidth * b.clientHeight - a.clientWidth * a.clientHeight; });
    return pool[0] || null;
  }
  function attachPlayer(v) {
    if (v.__telaP) return; v.__telaP = true;
    function post(kind) { try { window.top.postMessage({ __telaWatch: kind, time: v.currentTime, duration: v.duration || 0 }, "*"); } catch (e) {} }
    setInterval(function () { if (!v.paused && v.duration) post("progress"); }, 5000);
    v.addEventListener("pause", function () { post("progress"); });
    v.addEventListener("seeked", function () { post("progress"); });
    v.addEventListener("ended", function () { post("ended"); });
    window.addEventListener("message", function (e) {
      var d = e.data; if (!d || !d.__telaWatchResume) return;
      var seek = function () {
        if (v.duration && d.time > 5 && d.time < v.duration - 5 && Math.abs(v.currentTime - d.time) > 3) v.currentTime = d.time;
      };
      if (v.readyState >= 1) seek(); else v.addEventListener("loadedmetadata", seek, { once: true });
    });
    post("hello"); // announce so the host can send a resume point
  }
  var scan = function () { var v = pickVideo(); if (v) attachPlayer(v); };
  setInterval(scan, 3000);
  try { new MutationObserver(scan).observe(document.documentElement, { childList: true, subtree: true }); } catch (e) {}
  scan();

  // ── Host role: top frame owns metadata + storage; talks to player frames ─
  if (isTop) {
    window.addEventListener("message", function (e) {
      var d = e.data; if (!d || !d.__telaWatch) return;
      if (d.__telaWatch === "ended") { var a = loadAll(); delete a[itemKey()]; saveAll(a); return; }
      if (d.__telaWatch === "progress" && d.duration) recordProgress(d.time, d.duration);
      // Offer the saved resume point back to whichever frame reported in.
      var rec = loadAll()[itemKey()];
      if (rec && rec.time > 5 && e.source) { try { e.source.postMessage({ __telaWatchResume: true, time: rec.time }, "*"); } catch (x) {} }
    });

    // UI (host frame only).
    GM_addStyle(
      "#tela-wt-panel{position:fixed;right:16px;top:16px;z-index:2147483647;width:340px;max-height:70vh;overflow:auto;" +
      "background:#151515;color:#eee;font:14px system-ui,sans-serif;border:1px solid #333;border-radius:12px;padding:8px;box-shadow:0 8px 30px #0008}" +
      "#tela-wt-panel h3{margin:6px 8px 10px;font-size:15px}#tela-wt-panel a{display:block;color:#eee;text-decoration:none;padding:8px 10px;border-radius:8px}" +
      "#tela-wt-panel a:hover{background:#262626}#tela-wt-panel .sub{color:#999;font-size:12px;margin-top:2px}" +
      "#tela-wt-panel .bar{height:3px;background:#333;border-radius:2px;margin-top:6px}#tela-wt-panel .bar>i{display:block;height:3px;background:#4caf7d;border-radius:2px}"
    );
    function panel() {
      var old = document.getElementById("tela-wt-panel"); if (old) { old.remove(); return; }
      var all = loadAll();
      var items = Object.keys(all).map(function (k) { return all[k]; }).sort(function (a, b) { return b.ts - a.ts; });
      var p = document.createElement("div"); p.id = "tela-wt-panel"; p.innerHTML = "<h3>Continue watching</h3>";
      if (!items.length) p.innerHTML += "<div class=sub style='padding:8px 10px'>Nothing tracked yet.</div>";
      items.slice(0, 40).forEach(function (it) {
        var a = document.createElement("a"); a.href = it.url;
        a.innerHTML = "<div>" + (it.title || it.url).replace(/</g, "&lt;") + "</div>" +
          "<div class=sub>" + fmt(it.time) + " / " + fmt(it.duration) + " · " + it.host + "</div>" +
          "<div class=bar><i style='width:" + Math.min(100, (it.time / it.duration) * 100) + "%'></i></div>";
        p.appendChild(a);
      });
      (document.body || document.documentElement).appendChild(p);
    }
    GM_registerMenuCommand("Continue watching…", panel);
    window.addEventListener("keydown", function (e) { if (e.altKey && (e.key === "w" || e.key === "W")) { e.preventDefault(); panel(); } });
  }
})();
