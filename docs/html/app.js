(function () {
  const btn = document.querySelector(".menu-btn");
  const nav = document.querySelector("nav.side");
  if (btn && nav) {
    btn.addEventListener("click", function () {
      nav.classList.toggle("open");
      btn.setAttribute("aria-expanded", nav.classList.contains("open") ? "true" : "false");
    });
    document.addEventListener("click", function (event) {
      if (!nav.classList.contains("open")) return;
      if (nav.contains(event.target) || btn.contains(event.target)) return;
      nav.classList.remove("open");
      btn.setAttribute("aria-expanded", "false");
    });
  }

  document.querySelectorAll(".code").forEach(function (block) {
    const button = block.querySelector(".copy");
    const pre = block.querySelector("pre");
    if (!button || !pre) return;
    button.addEventListener("click", async function () {
      try {
        await navigator.clipboard.writeText(pre.innerText);
        button.textContent = "Copied";
        setTimeout(function () { button.textContent = "Copy"; }, 1400);
      } catch (err) {
        button.textContent = "Copy failed";
      }
    });
  });
})();
