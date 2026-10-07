// Small page behaviors. Nothing here talks to a server.
(function () {
  "use strict";
  // Respect "Reduce Motion": don't autoplay the hero film; show the poster instead.
  var reduce = window.matchMedia && window.matchMedia("(prefers-reduced-motion: reduce)").matches;
  document.querySelectorAll("video[autoplay]").forEach(function (v) {
    if (reduce) {
      v.removeAttribute("autoplay");
      v.pause();
      v.setAttribute("controls", "");
      return;
    }
    // If the film files aren't deployed, the poster stays: hide the broken-media state.
    v.addEventListener("error", function () { v.removeAttribute("autoplay"); }, true);
  });
})();
