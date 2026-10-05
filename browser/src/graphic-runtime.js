// The GodwinMix graphic runtime, injected into a page in graphic mode.
//
// The mixer sends the whole state of the graphic as one object:
//   {"fields": {"name": "Ada Lovelace", "accent": "#c8102e"}, "cue": "in"}
// and this applies it to the page, so a graphic needs no script of its own:
//
//   * every field becomes a CSS variable on <html>: var(--accent)
//   * an element with data-field="name" shows that field as its text
//     (an <img data-field="logo"> takes it as its src)
//   * <html> carries the class gmx-in while the graphic is on air and
//     gmx-out once it has been taken off, so CSS transitions do the moving
//   * a script that wants more listens for gmx:update, gmx:in and gmx:out
//     on window, or reads window.gmx.fields and window.gmx.cue
(function () {
  if (window.__gmxApply) return;
  var root = document.documentElement;
  var gmx = (window.gmx = window.gmx || { fields: {}, cue: "" });
  function fire(name, detail) {
    try { window.dispatchEvent(new CustomEvent(name, { detail: detail })); } catch (e) {}
  }
  function show(name, value) {
    var text = value === null || value === undefined ? "" : String(value);
    root.style.setProperty("--" + name, text);
    var els = document.querySelectorAll('[data-field="' + name + '"]');
    for (var i = 0; i < els.length; i++) {
      if (els[i].tagName === "IMG") els[i].setAttribute("src", text);
      else els[i].textContent = text;
    }
  }
  window.__gmxApply = function (state) {
    var fields = (state && state.fields) || {};
    var changed = false;
    // Every field is shown every time, changed or not: a state that arrived
    // while the page was still loading found no elements to fill.
    for (var k in fields) {
      if (gmx.fields[k] !== fields[k]) changed = true;
      show(k, fields[k]);
    }
    gmx.fields = Object.assign({}, gmx.fields, fields);
    if (changed) fire("gmx:update", gmx.fields);
    var cue = state && state.cue;
    if (cue === "in" || cue === "out") {
      root.classList.toggle("gmx-in", cue === "in");
      root.classList.toggle("gmx-out", cue === "out");
      if (cue !== gmx.cue) { gmx.cue = cue; fire("gmx:" + cue, gmx.fields); }
    }
  };
})();
