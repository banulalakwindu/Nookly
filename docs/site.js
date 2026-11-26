// Shared by the home page and the taskbar page.

// Point the download buttons straight at the latest installer.
fetch("https://api.github.com/repos/banulalakwindu/Nookly/releases/latest")
  .then(function (r) { return r.ok ? r.json() : null; })
  .then(function (rel) {
    if (!rel) return;
    var a = (rel.assets || []).find(function (x) { return /^NooklySetup.*\.exe$/i.test(x.name); });
    if (a) document.querySelectorAll("[data-dl]").forEach(function (el) { el.href = a.browser_download_url; });
    var mb = a ? " · " + (a.size / 1048576).toFixed(1) + " MB" : "";
    var m = document.getElementById("dl-meta");
    if (m) m.textContent = "Version " + String(rel.tag_name || "").replace(/^v/, "") + mb + " · Windows 10 and 11";
  })
  .catch(function () {});

// Click a screenshot to see it large.
(function () {
  var lb = document.getElementById("lb");
  if (!lb) return;
  document.querySelectorAll(".card img, div.shot img").forEach(function (img) {
    img.addEventListener("click", function () { lb.querySelector("img").src = img.dataset.full || img.src; lb.classList.add("on"); });
  });
  lb.addEventListener("click", function () { lb.classList.remove("on"); });
  document.addEventListener("keydown", function (e) { if (e.key === "Escape") lb.classList.remove("on"); });
})();
