const scrollThreshold = 180;

function ensureScrollTopButton() {
  const button = document.createElement("button");
  button.type = "button";
  button.className = "scroll-top-button";
  button.setAttribute("aria-label", "Scroll back to top");
  button.setAttribute("data-visible", "false");
  button.innerHTML = `
    <svg viewBox="0 0 24 24" aria-hidden="true" width="20" height="20" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
      <path d="M12 19V5"></path>
      <path d="m5 12 7-7 7 7"></path>
    </svg>
  `;

  const updateVisibility = () => {
    const shouldShow = window.scrollY > scrollThreshold;
    button.setAttribute("data-visible", shouldShow ? "true" : "false");
  };

  button.addEventListener("click", () => {
    window.scrollTo({ top: 0, behavior: "smooth" });
  });

  window.addEventListener("scroll", updateVisibility, { passive: true });
  window.addEventListener("load", updateVisibility);
  updateVisibility();

  document.body.appendChild(button);
}

if (document.readyState === "loading") {
  document.addEventListener("DOMContentLoaded", ensureScrollTopButton, { once: true });
} else {
  ensureScrollTopButton();
}
