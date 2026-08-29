(() => {
  const DRAWER_BREAKPOINT = "(max-width: 76.234375em)";
  const ANALYTICS_ENDPOINT = "/api/events";
  const ANALYTICS_SCHEMA = Object.freeze({
    cta_select: {
      action: ["open_quickstart", "compare_markdownlint", "open_playground", "install"],
      location: ["hero", "next"],
    },
    command_copy: {
      command: ["uvx_check"],
      result: ["success", "failure"],
    },
    playground_ready: { source: ["default", "shared"] },
    playground_example: { example: ["common", "headings", "links", "clean"] },
    playground_fix: {
      scope: ["single", "all"],
      outcome: ["clean", "remaining", "unchanged"],
    },
    playground_config: {
      flavor: ["standard", "mkdocs", "mdx", "pandoc", "quarto", "obsidian", "kramdown", "azure_devops", "myst", "hugo", "mdg"],
      disabled: ["0", "1", "2_4", "5_plus"],
      line_length: ["under_80", "80", "81_120", "over_120"],
    },
    playground_share: { result: ["success", "failure", "too_large"] },
    playground_error: { stage: ["load", "lint", "config", "share"] },
  });

  function analyticsValue(schema, value) {
    const normalized = String(value ?? "");
    return schema.includes(normalized) ? normalized : null;
  }

  function track(eventName, properties = {}) {
    const schema = ANALYTICS_SCHEMA[eventName];
    if (!schema) return false;

    const safeProperties = {};
    for (const [key, allowed] of Object.entries(schema)) {
      const value = analyticsValue(allowed, properties[key]);
      if (value === null) return false;
      safeProperties[key] = value;
    }

    const payload = { event: eventName, properties: safeProperties };
    window.dispatchEvent(new CustomEvent("rumdl:analytics", { detail: payload }));

    const hostname = window.location.hostname;
    const isProduction = hostname === "rumdl.dev" || hostname.endsWith(".rumdl.dev");
    if (!isProduction) return true;

    const body = JSON.stringify(payload);
    const blob = new Blob([body], { type: "application/json" });
    const queued = typeof navigator.sendBeacon === "function"
      ? navigator.sendBeacon(ANALYTICS_ENDPOINT, blob)
      : false;

    if (!queued) {
      fetch(ANALYTICS_ENDPOINT, {
        method: "POST",
        headers: { "content-type": "application/json" },
        body,
        credentials: "same-origin",
        keepalive: true,
      }).catch(() => {});
    }
    return true;
  }

  window.rumdlAnalytics = Object.freeze({ track });

  function enhanceAnalytics(root = document) {
    root.querySelectorAll("[data-rm-event]").forEach((element) => {
      if (element.dataset.rmEventReady === "true") return;
      element.dataset.rmEventReady = "true";
      element.addEventListener("click", () => {
        track(element.dataset.rmEvent, {
          action: element.dataset.rmAction,
          location: element.dataset.rmLocation,
        });
      });
    });
  }

  function enhanceCopyButtons(root = document) {
    root.querySelectorAll("[data-rm-copy]").forEach((button) => {
      if (button.dataset.rmReady === "true") return;
      button.dataset.rmReady = "true";
      const idleLabel = button.textContent;
      button.addEventListener("click", async () => {
        const text = button.dataset.rmCopy || "";
        const status = button.parentElement?.querySelector(".rm-copy-status");
        try {
          await navigator.clipboard.writeText(text);
          button.textContent = "Copied";
          if (status) status.textContent = `${text} copied to clipboard.`;
          track("command_copy", { command: button.dataset.rmCommand, result: "success" });
        } catch {
          if (status) status.textContent = `Copy failed. Select the command manually: ${text}`;
          track("command_copy", { command: button.dataset.rmCommand, result: "failure" });
        }
        window.setTimeout(() => {
          button.textContent = idleLabel;
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
    enhanceAnalytics(root);
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
