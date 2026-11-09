// Sanaga's password and AutoFill helper, run at the start of every page (top
// frame). It notices sign-ins (to offer saving them), asks Sanaga for the saved
// accounts of this site, shows them under the field, and fills fields when
// Sanaga hands it a password. Passwords never stay in the page script.
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

  // --- saving: a sign-in is sent (form, button, Enter, leaving the page) ---
  var last = '';
  function capture(scope) {
    var s = scope && scope.querySelector ? scope : document;
    var p = pwField(s) || pwField(document);
    if (!p || !p.value) return;
    var u = userField(p.form || s) || userField(document);
    var user = u && u.value ? u.value : rememberedUser();
    var msg = 'pw:save\n' + e(location.origin) + '\n' + e(user) + '\n' + e(p.value);
    if (msg === last) return;
    last = msg;
    send(msg);
  }
  document.addEventListener('submit', function (ev) { capture(ev.target); }, true);
  document.addEventListener('click', function (ev) {
    var b = ev.target.closest && ev.target.closest('button,input[type=submit],input[type=button],[role=button],a');
    if (b) capture(b.form || b.closest('form') || document);
  }, true);
  document.addEventListener('keydown', function (ev) {
    if (ev.key === 'Enter' && ev.target && ev.target.tagName === 'INPUT') capture(ev.target.form || document);
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

  // --- filling: ask once per address, again when sign-in fields appear later ---
  var users = null, asked = '', box = null;
  function ask() {
    // asked again when the address changes or new sign-in fields appear (the
    // password screen of a two-step sign-in often keeps the same address)
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

  // the list of saved accounts under the field (names only: the password
  // comes from Sanaga after a real click)
  function hide() { if (box) { box.remove(); box = null; } }
  function show(field) {
    hide();
    if (!users || !users.length) return;
    var r = field.getBoundingClientRect();
    box = document.createElement('div');
    box.style.cssText = 'all:initial;position:fixed;z-index:2147483647;left:' + r.left + 'px;top:' + (r.bottom + 4) + 'px;min-width:' + Math.max(r.width, 240) + 'px';
    var sh = box.attachShadow({ mode: 'closed' });
    var dark = matchMedia('(prefers-color-scheme: dark)').matches;
    sh.innerHTML = '<style>' +
      '.l{font:14px system-ui,sans-serif;background:' + (dark ? '#2b2b2e' : '#fff') + ';color:' + (dark ? '#eee' : '#1d1d1f') + ';border:1px solid ' + (dark ? '#444' : '#d2d2d7') + ';border-radius:10px;box-shadow:0 6px 24px rgba(0,0,0,.18);padding:4px}' +
      'button{display:flex;gap:10px;align-items:center;width:100%;border:0;background:none;color:inherit;font:inherit;text-align:left;padding:8px 10px;border-radius:7px;cursor:pointer}' +
      'button:hover{background:' + (dark ? '#3a3a3e' : '#eef3ff') + '}span{flex:1}small{opacity:.55}svg{flex:none;opacity:.75}' +
      '</style><div class="l"></div>';
    var list = sh.querySelector('.l');
    users.forEach(function (u) {
      var b = document.createElement('button');
      b.innerHTML = '<svg width="16" height="16" viewBox="0 0 16 16"><circle cx="5" cy="8" r="3.2" fill="none" stroke="currentColor" stroke-width="1.6"/><path d="M8 8h6.5M12 8v2.6M14.2 8v2" stroke="currentColor" stroke-width="1.6" fill="none"/></svg>';
      var t = document.createElement('span');
      t.textContent = u || '—';
      var dots = document.createElement('small');
      dots.textContent = '••••••••';
      b.appendChild(t);
      b.appendChild(dots);
      b.addEventListener('mousedown', function (ev) {
        ev.preventDefault();
        if (!ev.isTrusted) return;
        send('pw:fill\n' + e(location.origin) + '\n' + e(u));
        hide();
      });
      list.appendChild(b);
    });
    document.documentElement.appendChild(box);
  }
  function isLoginField(t) { return t && t.tagName === 'INPUT' && (t.type === 'password' || t.matches(USER)); }
  document.addEventListener('focusin', function (ev) {
    if (isLoginField(ev.target)) { ask(); if (!ev.target.value) show(ev.target); } else hide();
  }, true);
  document.addEventListener('input', function (ev) { if (ev.isTrusted) hide(); }, true);
  document.addEventListener('mousedown', function (ev) { if (box && ev.target !== box) hide(); }, true);
  window.addEventListener('scroll', hide, true);
  document.addEventListener('keydown', function (ev) { if (ev.key === 'Escape') hide(); }, true);
  window.__sanagaLogins = function (list) {
    users = list;
    var a = document.activeElement;
    if (isLoginField(a) && !a.value) show(a);
  };

  function setv(el, v) {
    if (!el || v == null || v === '') return;
    var d = Object.getOwnPropertyDescriptor(Object.getPrototypeOf(el), 'value');
    if (d && d.set) d.set.call(el, v); else el.value = v;
    el.dispatchEvent(new Event('input', { bubbles: true }));
    el.dispatchEvent(new Event('change', { bubbles: true }));
    el.style.backgroundColor = 'rgba(250, 210, 80, .25)';
  }
  // onlyEmpty: never overwrite what the person typed
  window.__sanagaFill = function (u, p, onlyEmpty) {
    var pw = pwField();
    var uf = userField(pw ? (pw.form || document) : document);
    if (uf && !(onlyEmpty && uf.value)) { setv(uf, u); rememberUser(u); }
    if (pw && !(onlyEmpty && pw.value)) setv(pw, p);
    hide();
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
