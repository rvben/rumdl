---
description: "Try fast Markdown linting and formatting with your existing markdownlint configuration, automatic fixes, and one engine for CLI, editors, and CI."
icon: lucide/hash
hide:
  - navigation
  - toc
---

<!-- rumdl-disable MD041 -->
<div class="rm-home" markdown>
<nav class="rm-home-nav" aria-label="Primary site navigation">
<a class="rm-home-nav__docs" href="https://rumdl.dev/getting-started/quickstart/">Documentation</a>
<a href="https://rumdl.dev/playground/">Playground</a>
<a href="https://rumdl.dev/getting-started/installation/">Installation</a>
</nav>
<section class="rm-hero" aria-labelledby="rm-hero-title">
<div class="rm-hero__copy">
<h1 id="rm-hero-title">Clean Markdown. Fast.</h1>
<p class="rm-hero__lede">Run a read-only check without a permanent install. Use rumdl defaults or keep your markdownlint configuration; one native binary finds, explains, and fixes Markdown across your terminal, editor, and CI.</p>
<p class="rm-command-label">Run a read-only check now</p>
<div class="rm-install rm-install--primary" aria-label="Read-only trial command">
<code>uvx rumdl check .</code>
<button type="button" class="rm-copy" data-rm-copy="uvx rumdl check ." data-rm-command="uvx_check" aria-label="Copy read-only trial command">Copy command</button>
<span class="rm-copy-status" aria-live="polite"></span>
</div>
<div class="rm-actions" aria-label="Learn more">
<a class="rm-button rm-button--secondary" href="https://rumdl.dev/getting-started/quickstart/" data-rm-event="cta_select" data-rm-action="repository_trial" data-rm-location="hero">60-second quickstart</a>
<a class="rm-button rm-button--secondary" href="https://rumdl.dev/markdownlint-comparison/" data-rm-event="cta_select" data-rm-action="compare_markdownlint" data-rm-location="hero">Compare with markdownlint</a>
</div>
<p class="rm-hero__aside">The first run does not change files. rumdl works immediately and discovers common markdownlint configuration automatically.</p>
<div class="rm-hero__alternatives" aria-label="Other ways to start">
<a href="https://rumdl.dev/playground/" data-rm-event="cta_select" data-rm-action="open_playground" data-rm-location="hero">Try in your browser</a>
<a href="https://rumdl.dev/getting-started/installation/" data-rm-event="cta_select" data-rm-action="install" data-rm-location="hero">Install rumdl</a>
</div>
</div>
<figure class="rm-terminal-shot">
<div class="rm-terminal-shot__window">
<div class="rm-terminal-shot__bar" aria-hidden="true">
<span class="rm-terminal-shot__controls"><i></i><i></i><i></i></span>
<strong>rumdl — docs</strong>
<span class="rm-terminal-shot__shell">zsh</span>
</div>
<img src="images/homepage-terminal.png" width="1068" height="342" alt="Terminal running uvx rumdl check dot, reporting two Markdown issues, and suggesting rumdl fmt" fetchpriority="high" decoding="sync">
</div>
<figcaption>Captured from an actual <code>uvx rumdl check .</code> run.</figcaption>
</figure>
</section>
<p class="rm-engine-line"><strong>One engine, every workflow.</strong> Run rumdl in the CLI, through its built-in language server, in CI, or as WebAssembly in the browser.</p>
<p class="rm-adopters">Trusted in public repositories including <strong>Firefox</strong>, <strong>Docker Docs</strong>, <strong>Apache Lucene</strong>, <strong>PyO3</strong>, and <strong>Rustlings</strong>. <a href="https://github.com/rvben/rumdl#used-by">See the adopter list</a>.</p>
<section class="rm-section rm-workflow" aria-labelledby="rm-workflow-title">
<div class="rm-section__intro">
<h2 id="rm-workflow-title">From warning to clean Markdown in one loop</h2>
<p>Diagnostics stay precise and fixes stay close to the source, whether you are checking one file or an entire repository.</p>
</div>
<ol class="rm-flow">
<li>
<code>rumdl check .</code>
<div><strong>Find the exact problem</strong><span>Rule, file, line, column, and a clear explanation.</span></div>
</li>
<li>
<code>rumdl check --fix .</code>
<div><strong>Apply safe fixes</strong><span>Fix what can be automated and keep manual work explicit.</span></div>
</li>
<li>
<code>rumdl fmt .</code>
<div><strong>Keep it consistent</strong><span>Format Markdown predictably across local and CI workflows.</span></div>
</li>
</ol>
</section>
<section class="rm-section rm-performance" id="performance" aria-labelledby="rm-performance-title">
<div class="rm-section__intro">
<h2 id="rm-performance-title">Fast enough to stay in the loop</h2>
<p>Cold-start benchmark on the Rust Book repository: 478 Markdown files with application caches disabled.</p>
</div>
<div class="rm-benchmark">
<table>
<thead><tr><th scope="col">Linter</th><th scope="col">Mean time</th><th scope="col">Relative to rumdl</th></tr></thead>
<tbody>
<tr class="rm-benchmark__winner"><th scope="row">rumdl</th><td>217 ms</td><td>1.0×</td></tr>
<tr><th scope="row">markdownlint-cli2</th><td>2.2 s</td><td>10.2×</td></tr>
<tr><th scope="row">markdownlint-cli</th><td>2.7 s</td><td>12.5×</td></tr>
</tbody>
</table>
<p>Repeated rumdl checks can also skip unchanged files. <a href="https://rumdl.dev/comparison/#performance">See the method and results</a>.</p>
</div>
</section>
<section class="rm-section rm-capabilities" aria-labelledby="rm-capabilities-title">
<div class="rm-section__intro">
<h2 id="rm-capabilities-title">Meet your Markdown where it lives</h2>
<p>Keep one rule set across formats and tools without adding a runtime to every environment.</p>
</div>
<div class="rm-capability-list">
<p><strong><!-- RULE_COUNT -->82<!-- /RULE_COUNT --> lint rules</strong><span>Broad markdownlint compatibility with direct, configurable diagnostics.</span></p>
<p><strong>Multiple Markdown flavors</strong><span>GFM, MkDocs, MDX, Quarto, MyST, and more.</span></p>
<p><strong>Editor-native feedback</strong><span>A built-in language server with diagnostics, quick fixes, and formatting.</span></p>
<p><strong>CI-ready output</strong><span>GitHub, GitLab, Azure, SARIF, JUnit, JSON, and other structured formats.</span></p>
</div>
</section>
<nav class="rm-next" aria-label="Choose your next step">
<div>
<h2>Start with the shortest path</h2>
<p>Check a repository without a permanent install, then change your workflow only if the results earn it.</p>
</div>
<div class="rm-next__action">
<div class="rm-next__primary">
<div class="rm-next__primary-copy"><strong>Run the repository trial</strong><span>No install, no changed files, and your existing markdownlint configuration is discovered automatically.</span></div>
<div class="rm-install rm-install--primary" aria-label="Read-only trial command">
<code>uvx rumdl check .</code>
<button type="button" class="rm-copy" data-rm-copy="uvx rumdl check ." data-rm-command="uvx_check" aria-label="Copy read-only trial command">Copy command</button>
<span class="rm-copy-status" aria-live="polite"></span>
</div>
<a class="rm-button rm-button--secondary" href="https://rumdl.dev/getting-started/quickstart/" data-rm-event="cta_select" data-rm-action="repository_trial" data-rm-location="next">Open the 60-second quickstart</a>
</div>
<div class="rm-next__links">
<a href="https://rumdl.dev/playground/" data-rm-event="cta_select" data-rm-action="open_playground" data-rm-location="next"><strong>Browser playground</strong><span>Try the same engine with no install</span></a>
<a href="https://rumdl.dev/markdownlint-comparison/" data-rm-event="cta_select" data-rm-action="compare_markdownlint" data-rm-location="next"><strong>Migrate from markdownlint</strong><span>Keep your configuration for the first run</span></a>
</div>
</div>
</nav>
</div>
