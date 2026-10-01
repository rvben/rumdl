// Leave the existing search usable if question search cannot load.
import('./docs-search-ui.js').catch(() => {});
