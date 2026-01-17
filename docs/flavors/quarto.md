# Quarto Flavor

For projects using [Quarto](https://quarto.org/) or RMarkdown for scientific publishing.

## Supported Patterns

### Cell Options

Quarto code cell options using `#|` syntax:

````markdown
```{python}
#| label: fig-example
#| fig-cap: "Example figure"
import matplotlib.pyplot as plt
plt.plot([1, 2, 3])
```
````

**Affected rules**: MD038 (code spans), MD040 (fenced code language)

### Executable Code Blocks

Code blocks with language in braces:

````markdown
```{r}
summary(data)
```

```{python}
print("Hello")
```
````

**Affected rules**: MD040 (fenced code language)

### Pandoc Citations

Quarto supports Pandoc citation syntax:

```markdown
According to @smith2020, the results show...

Multiple citations [@smith2020; @jones2021] confirm this.

Suppress author: [-@smith2020] showed...

In-text: @smith2020 [p. 42] argues that...
```

**Affected rules**: MD042 (empty links), MD051 (link fragments), MD052 (reference links)

### Shortcodes

Quarto/Hugo shortcodes are recognized:

```markdown
{{< video https://youtube.com/watch?v=xxx >}}

{{< include _content.qmd >}}

{{% callout note %}}
This is a callout.
{{% /callout %}}
```

**Affected rules**: MD034 (bare URLs - URLs in shortcodes not flagged), MD042, MD051, MD052

### Div Blocks and Callouts

Quarto div syntax with attributes:

```markdown
::: {.callout-note}
This is a note callout.
:::

::: {.column-margin}
Marginal content here.
:::

::: {#fig-layout layout-ncol=2}
![Caption A](imageA.png)

![Caption B](imageB.png)
:::
```

**Affected rules**: MD031 (blanks around fences)

### Math Blocks

LaTeX math blocks are recognized and excluded from emphasis checking:

```markdown
$$
E = mc^2
$$

Inline math $\alpha + \beta$ is also recognized.
```

**Affected rules**: MD037 (no space in emphasis), MD049 (emphasis style), MD050 (strong style)

## Rule Behavior Changes

| Rule  | Standard Behavior           | Quarto Behavior                          |
| ----- | --------------------------- | ---------------------------------------- |
| MD034 | Flag all bare URLs          | Skip URLs inside shortcodes              |
| MD037 | Check emphasis spacing      | Skip math blocks                         |
| MD038 | Check all code spans        | Handle Quarto-specific syntax            |
| MD040 | Standard language detection | Recognize `{language}` and `#\|` options |
| MD042 | Flag empty links            | Skip citations and shortcodes            |
| MD049 | Check emphasis consistency  | Skip math blocks                         |
| MD050 | Check strong consistency    | Skip math blocks                         |
| MD051 | Validate link fragments     | Skip citations and shortcodes            |
| MD052 | Flag undefined references   | Skip citations and shortcodes            |

## Limitations

- Complex Pandoc filter syntax may not be fully recognized
- YAML front matter extensions are parsed as standard YAML

## Configuration

```toml
[global]
flavor = "quarto"
```

Or auto-detect by file extension:

```toml
[per-file-flavor]
"**/*.qmd" = "quarto"
"**/*.Rmd" = "quarto"
```

Note: `.qmd` and `.Rmd` files are auto-detected as Quarto flavor by default.

## When to Use

Use the Quarto flavor when:

- Writing Quarto documents (`.qmd`)
- Writing RMarkdown documents (`.Rmd`)
- Creating scientific publications with citations
- Using Jupyter notebooks converted to Quarto

## See Also

- [Flavors Overview](../flavors.md) - Compare all flavors
- [Quarto Documentation](https://quarto.org/)
