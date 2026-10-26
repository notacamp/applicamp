// Applies the saved theme and density to <html> before first paint, so a
// reload never flashes the wrong theme. A classic script loaded from
// index.html (no inline scripts). Keep in step with src/lib/storage/prefs.ts
// (the same key prefix) and @notacamp/ui's theme settings.
(function () {
  var prefix = "applicamp.pref.";
  var allowed = {
    theme: ["system", "light", "dark"],
    density: ["comfortable", "compact"],
  };
  function read(key) {
    try {
      return globalThis.localStorage.getItem(prefix + key);
    } catch {
      return null;
    }
  }
  function pick(name, value) {
    return allowed[name].includes(value) ? value : allowed[name][0];
  }
  var root = document.documentElement;
  root.dataset.theme = pick("theme", read("theme"));
  root.dataset.density = pick("density", read("density"));
})();
