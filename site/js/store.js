// App Store state for every call to action on the site.
// Flip `live` to true when Apple approves the app: every "Get notified" block
// becomes the official App Store badges, and store links point at the listing.
// The HTML defaults to the not-live state, so the page is correct with JS off.
const STORE = {
  live: false,
  ios: "https://apps.apple.com/us/app/nofriction-record-your-life/id6818838861",
  mac: "https://apps.apple.com/us/app/nofriction-record-your-life/id6818838861?mt=12",
  testflight: "https://testflight.apple.com/join/4JnJD3Fk"
};

(function () {
  "use strict";
  // Store links always come from STORE, so one edit updates every page.
  document.querySelectorAll("[data-store-href]").forEach(function (a) {
    var key = a.getAttribute("data-store-href");
    if (STORE[key]) a.setAttribute("href", STORE[key]);
  });
  if (!STORE.live) return;
  document.querySelectorAll('[data-store="notify"]').forEach(function (el) { el.hidden = true; });
  document.querySelectorAll('[data-store="live"]').forEach(function (el) { el.hidden = false; });
  document.querySelectorAll("[data-store-text]").forEach(function (el) {
    el.textContent = el.getAttribute("data-store-text");
  });
})();
