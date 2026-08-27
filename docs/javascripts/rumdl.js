(() => {
  const DRAWER_BREAKPOINT = "(max-width: 76.234375em)";

  function enhanceCopyButtons(root = document) {
    root.querySelectorAll("[data-rm-copy]").forEach((button) => {
      if (button.dataset.rmReady === "true") return;
      button.dataset.rmReady = "true";
      button.addEventListener("click", async () => {
        const text = button.dataset.rmCopy || "";
        const status = button.parentElement?.querySelector(".rm-copy-status");
        try {
          await navigator.clipboard.writeText(text);
          button.textContent = "Copied";
          if (status) status.textContent = `${text} copied to clipboard.`;
        } catch {
          if (status) status.textContent = `Copy failed. Select the command manually: ${text}`;
        }
        window.setTimeout(() => {
          button.textContent = "Copy";
        }, 1800);
      });
    });
  }

  function enhancePalette(root = document) {
    root.querySelectorAll("[data-md-component='palette']").forEach((form) => {
      form.querySelectorAll("input.md-option").forEach((input) => {
        input.tabIndex = -1;
      });
      form.querySelectorAll("label[for]").forEach((label) => {
        label.setAttribute("role", "button");
        if (label.title) label.setAttribute("aria-label", label.title);
        label.tabIndex = 0;
        if (label.dataset.rmReady === "true") return;
        label.dataset.rmReady = "true";
        label.addEventListener("keydown", (event) => {
          if (event.key === "Enter" || event.key === " ") {
            event.preventDefault();
            label.click();
          }
        });
      });
    });
  }

  function manageDrawerFocus(root = document) {
    const drawer = root.getElementById("__drawer");
    const sidebar = root.querySelector(".md-sidebar--primary");
    if (!drawer || !sidebar) return;

    const media = window.matchMedia(DRAWER_BREAKPOINT);
    const sync = () => {
      const shouldBeInert = media.matches && !drawer.checked;
      sidebar.inert = shouldBeInert;
      if (shouldBeInert) sidebar.setAttribute("aria-hidden", "true");
      else sidebar.removeAttribute("aria-hidden");
    };

    if (drawer.dataset.rmReady !== "true") {
      drawer.dataset.rmReady = "true";
      drawer.addEventListener("change", sync);
      media.addEventListener("change", sync);
    }
    sync();
  }

  function enhance(root = document) {
    enhanceCopyButtons(root);
    enhancePalette(root);
    manageDrawerFocus(root);
  }

  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", () => enhance(), { once: true });
  } else {
    enhance();
  }

  if (typeof window.document$ !== "undefined") {
    window.document$.subscribe(() => enhance());
  }
})();
