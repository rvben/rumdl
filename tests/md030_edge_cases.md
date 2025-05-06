<!-- MD030 Edge Cases Test File -->

# MD030 Edge Cases

---

<!-- 1. Single-line ordered and unordered lists -->

1. one
2. two
3. three

- one
- two
- three

+ one
+ two
+ three

* one
* two
* three

---

<!-- 2. Multi-line (wrapped) list items -->

1. one
   continued
2. two
   continued

- one
  continued
- two
  continued

---

<!-- 3. Nested lists (various indentations) -->

- parent
    - child
        - grandchild
- parent2
    1. child
        1. grandchild

1. parent
    - child
    - child2
1. parent2
    1. child
    1. child2

---

<!-- 4. Blank lines within and after lists -->

- item 1

- item 2

- item 3

1. item 1

1. item 2

1. item 3

- item 1
  
  continued after blank
- item 2

---

<!-- 5. List items with too few, exact, and too many spaces after marker -->

-TooFew
- Exact
-  TooMany
1.TooFew
1. Exact
1.  TooMany

---

<!-- 6. List items with code blocks, blockquotes, and HTML blocks -->

- item
    ```
    code block
    ```
- item2
> - quoted list
>   - quoted nested
- <div>HTML block</div>

---

<!-- 7. List items with tabs after the marker -->

-\tone with tab
1.\ttwo with tab

---

<!-- 8. List items with only the marker (empty content) -->

-
1.

---

<!-- 9. List items with non-ASCII markers or content -->

- café
1. naïve

---

<!-- 10. Mixed marker styles -->

- item
+ item
* item

1. item
2. item

---

<!-- 11. List items inside blockquotes -->

> - quoted item
> 1. quoted ordered

---

<!-- 12. List items with front matter above -->

---
title: Test
---

- after front matter
1. after front matter

---

<!-- 13. List items with reference links, images, and inline HTML -->

- [link][ref]
- ![alt](img.png)
- <span>inline html</span>

[ref]: http://example.com 