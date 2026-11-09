// Sanaga's password and AutoFill helper, run at the start of every page, in
// Sanaga's own JavaScript world (the page's scripts can't see or replace it)
// and in the top frame only. It notices sign-ins (to offer saving them),
// tells Sanaga when sign-in fields appear (Sanaga then offers its saved
// accounts in its own bar), and fills the fields only when the person picked
// an account there. Passwords never stay in the script.
(function () {
  if (window.__sanagaPw) return;
  window.__sanagaPw = true;
  function send(m) { if (window.ipc) window.ipc.postMessage(m); }
  var e = encodeURIComponent;
  var USER = 'input[autocomplete~="username" i],input[autocomplete~="email" i],input[type=email],input[name*=user i],input[name*=login i],input[name*=mail i],input[id*=user i],input[id*=login i],input[id*=mail i]';
  function visible(el) { return !!el && el.offsetParent !== null && !el.disabled && el.type !== 'hidden'; }
  function pwField(scope) {
    var all = (scope || document).querySelectorAll('input[type=password]');
    for (var i = 0; i < all.length; i++) if (visible(all[i])) return all[i];
    return null;
  }
  function userField(scope) {
    var s = scope || document;
    var all = s.querySelectorAll(USER);
    for (var i = 0; i < all.length; i++) if (visible(all[i]) && all[i].type !== 'password') return all[i];
    // otherwise the text field just before the password field
    var p = pwField(s);
    if (p) {
      var ins = Array.prototype.slice.call(s.querySelectorAll('input[type=text],input:not([type])')).filter(visible);
      var before = ins.filter(function (x) { return x.compareDocumentPosition(p) & 4; });
      if (before.length) return before[before.length - 1];
    }
    return null;
  }
  // two-step sign-in (address first, password on the next screen): remember
  // the address for this tab
  function rememberUser(v) { try { if (v) sessionStorage.setItem('__sanagaUser', v); } catch (x) {} }
  function rememberedUser() { try { return sessionStorage.getItem('__sanagaUser') || ''; } catch (x) { return ''; } }
  document.addEventListener('change', function (ev) {
    var t = ev.target;
    if (t && t.matches && t.type !== 'password' && t.matches(USER)) rememberUser(t.value);
  }, true);

  // --- saving: a sign-in the person really sent (a real submit, click or
  // Enter, or leaving the page) with a password the person really typed (or
  // Sanaga filled): a page can't raise the "Save password?" question itself,
  // with values of its own choosing ---
  var typed = new WeakSet();
  document.addEventListener('input', function (ev) {
    var t = ev.target;
    if (ev.isTrusted && t && t.tagName === 'INPUT' && t.type === 'password') typed.add(t);
  }, true);
  var last = '';
  function capture(scope) {
    var s = scope && scope.querySelector ? scope : document;
    var p = pwField(s) || pwField(document);
    if (!p || !p.value || !typed.has(p)) return;
    var u = userField(p.form || s) || userField(document);
    var user = u && u.value ? u.value : rememberedUser();
    var msg = 'pw:save\n' + e(location.origin) + '\n' + e(user) + '\n' + e(p.value);
    if (msg === last) return;
    last = msg;
    send(msg);
  }
  document.addEventListener('submit', function (ev) { if (ev.isTrusted) capture(ev.target); }, true);
  document.addEventListener('click', function (ev) {
    if (!ev.isTrusted) return;
    var b = ev.target.closest && ev.target.closest('button,input[type=submit],input[type=button],[role=button],a');
    if (b) capture(b.form || b.closest('form') || document);
  }, true);
  document.addEventListener('keydown', function (ev) {
    if (ev.isTrusted && ev.key === 'Enter' && ev.target && ev.target.tagName === 'INPUT') capture(ev.target.form || document);
  }, true);
  window.addEventListener('pagehide', function () { capture(document); });

  // No "ping" requests telling sites which links you follow (<a ping>,
  // sent by Chrome and Safari; not by Firefox, nor Sanaga): the attribute
  // goes before the click reaches the link.
  ['mousedown', 'click', 'auxclick', 'keydown'].forEach(function (type) {
    document.addEventListener(type, function (ev) {
      var a = ev.target && ev.target.closest && ev.target.closest('a[ping], area[ping]');
      if (a) a.removeAttribute('ping');
    }, true);
  });

  // Text typed in a form and not sent yet: Sanaga keeps this tab awake (the
  // memory saver would lose it). Sent or emptied: it may sleep again.
  var dirty = false;
  function setDirty(d) { if (d !== dirty) { dirty = d; send(d ? 'form:dirty' : 'form:clean'); } }
  document.addEventListener('input', function (ev) {
    var t = ev.target;
    if (!ev.isTrusted || !t) return;
    var field = t.tagName === 'TEXTAREA' || t.isContentEditable || (t.tagName === 'INPUT' && !/^(button|submit|checkbox|radio|range|color|file|hidden|search)$/i.test(t.type));
    if (field) setDirty(String(t.value || t.textContent || '').length > 0);
  }, true);
  document.addEventListener('submit', function () { setDirty(false); }, true);

  // --- filling: tell Sanaga once per address, again when sign-in fields
  // appear later (the password screen of a two-step sign-in often keeps the
  // same address). Sanaga shows the saved accounts in its own bar; nothing
  // is filled before the person picks one there.
  var asked = '';
  function ask() {
    var p = pwField(), u = userField();
    if (!p && !u) return;
    var key = location.href + (p ? '|p' : '') + (u ? '|u' : '');
    if (asked === key) return;
    asked = key;
    send('pw:get\n' + e(location.origin));
  }
  // Light on busy pages (mail, social networks, videos): only added elements
  // are looked at (not every style or class change), only when one of them
  // holds a text field, at most once a second, and never in a hidden tab.
  // A field shown later in another way is found when it gets the focus.
  var timer = 0;
  function soon() { if (!timer && !document.hidden) timer = setTimeout(function () { timer = 0; ask(); }, 1000); }
  function hasField(n) {
    return n.nodeType === 1 && (n.tagName === 'INPUT' || (n.firstElementChild && n.getElementsByTagName('input').length > 0));
  }
  function added(list) {
    if (timer || document.hidden) return;
    for (var i = 0; i < list.length; i++) {
      var nodes = list[i].addedNodes;
      for (var j = 0; j < nodes.length; j++) if (hasField(nodes[j])) { soon(); return; }
    }
  }
  function start() {
    ask();
    new MutationObserver(added).observe(document.documentElement, { childList: true, subtree: true });
    document.addEventListener('visibilitychange', soon);
  }
  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', start); else start();
  document.addEventListener('focusin', function (ev) {
    var t = ev.target;
    if (t && t.tagName === 'INPUT' && (t.type === 'password' || t.matches(USER))) ask();
  }, true);

  function setv(el, v) {
    if (!el || v == null || v === '') return;
    var d = Object.getOwnPropertyDescriptor(Object.getPrototypeOf(el), 'value');
    if (d && d.set) d.set.call(el, v); else el.value = v;
    el.dispatchEvent(new Event('input', { bubbles: true }));
    el.dispatchEvent(new Event('change', { bubbles: true }));
    el.style.backgroundColor = 'rgba(250, 210, 80, .25)';
  }
  // Called by Sanaga after the person picked an account in Sanaga's bar.
  // The origin is checked again here, at the moment of filling, against the
  // top-level document: a page that changed in the meantime gets nothing,
  // and frames are never filled (this script runs in the top frame only).
  window.__sanagaFill = function (u, p, origin) {
    if (window.top !== window) return 'frame';
    if (location.origin !== origin) return 'origin';
    var pw = pwField();
    var uf = userField(pw ? (pw.form || document) : document);
    if (uf) { setv(uf, u); rememberUser(u); }
    if (pw) { setv(pw, p); typed.add(pw); }
    return pw || uf ? 'filled' : 'nofield';
  };

  // --- contact AutoFill (name, e-mail, phone, address) ---
  window.__sanagaContact = function (c) {
    var map = [['name', c.name, /name|nom/i], ['email', c.email, /mail/i], ['tel', c.phone, /tel|phone/i], ['street-address', c.address, /address|adresse|rue/i]];
    var n = 0;
    document.querySelectorAll('input,textarea').forEach(function (el) {
      if (el.type === 'password' || el.type === 'hidden' || el.value) return;
      var ac = (el.getAttribute('autocomplete') || '').toLowerCase();
      var key = el.name + ' ' + el.id + ' ' + (el.placeholder || '') + ' ' + el.type;
      for (var i = 0; i < map.length; i++) {
        var m = map[i];
        if (!m[1]) continue;
        if (ac.indexOf(m[0]) >= 0 || (m[0] === 'email' && el.type === 'email') || (m[0] === 'tel' && el.type === 'tel') || (!ac && m[2].test(key))) { setv(el, m[1]); n++; break; }
      }
    });
    return n;
  };
})();
