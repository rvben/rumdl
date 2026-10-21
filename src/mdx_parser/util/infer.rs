//! Infer things from events.
//!
//! Used to share between `to_html` and `to_mdast`.

use crate::mdx_parser::event::{Event, Kind, Name};
use crate::mdx_parser::mdast::AlignKind;
use alloc::{vec, vec::Vec};

/// Whether lists and list items are spread (loose), found in one pass.
#[derive(Debug)]
pub struct ListSpread {
    /// For each list and item enter, in order: `(enter, spread, loose)`.
    entries: Vec<(usize, bool, bool)>,
}

impl ListSpread {
    pub fn new(events: &[Event]) -> ListSpread {
        let mut entries: Vec<(usize, bool, bool)> = vec![];
        // Per open event: the entry of a list or item.
        let mut open: Vec<Option<usize>> = vec![];

        for (index, event) in events.iter().enumerate() {
            if event.kind == Kind::Enter {
                let entry = if matches!(event.name, Name::ListOrdered | Name::ListUnordered | Name::ListItem) {
                    entries.push((index, false, false));
                    Some(entries.len() - 1)
                } else {
                    None
                };
                open.push(entry);
                continue;
            }

            let entry = open.pop().flatten();

            if let Some(Some(parent)) = open.last() {
                let parent = *parent;
                let is_item = events[entries[parent].0].name == Name::ListItem;

                if event.name == Name::BlankLineEnding
                    && !(if is_item {
                        item_blank_after_prefix(events, index)
                    } else {
                        list_blank_after_empty(events, index)
                    })
                {
                    entries[parent].1 = true;
                    entries[parent].2 = true;
                }

                if let Some(entry) = entry {
                    if !is_item && events[entries[entry].0].name == Name::ListItem && entries[entry].1 {
                        entries[parent].2 = true;
                    }
                }
            }
        }

        ListSpread { entries }
    }

    fn entry(&self, enter: usize) -> (usize, bool, bool) {
        let position = self
            .entries
            .binary_search_by_key(&enter, |entry| entry.0)
            .expect("expected list or list item");
        self.entries[position]
    }

    /// Whether the list or item entered at `enter` is spread.
    pub fn spread(&self, enter: usize) -> bool {
        self.entry(enter).1
    }

    /// Whether the list entered at `enter` is spread or has a spread item.
    pub fn loose(&self, enter: usize) -> bool {
        self.entry(enter).2
    }
}

/// Whether the blank line ending at `index`, directly in a list, is right
/// after an empty item or block quote prefix.
fn list_blank_after_empty(events: &[Event], index: usize) -> bool {
    // Blank line directly after item, which is just a prefix.
    //
    // ```markdown
    // > | -␊
    //      ^
    //   | - a
    // ```
    //
    // Blank line at block quote prefix:
    //
    // ```markdown
    // > | * >␊
    //        ^
    //   | * a
    // ```
    let mut before = index - 2;

    if events[before].name == Name::ListItem {
        before -= 1;

        if events[before].name == Name::SpaceOrTab {
            before -= 2;
        }

        (events[before].name == Name::BlockQuote && events[before - 1].name == Name::BlockQuotePrefix)
            || events[before].name == Name::ListItemPrefix
    } else {
        false
    }
}

/// Whether the blank line ending at `index`, directly in a list item, is right
/// after its prefix.
fn item_blank_after_prefix(events: &[Event], index: usize) -> bool {
    // Blank line directly after a prefix:
    //
    // ```markdown
    // > | -␊
    //      ^
    //   |   a
    // ```
    let mut before = index - 2;

    if events[before].name == Name::SpaceOrTab {
        before -= 2;
    }

    events[before].name == Name::ListItemPrefix
}

/// Figure out the alignment of a GFM table.
pub fn gfm_table_align(events: &[Event], mut index: usize) -> Vec<AlignKind> {
    debug_assert!(matches!(events[index].name, Name::GfmTable), "expected table");
    let mut in_delimiter_row = false;
    let mut align = vec![];

    while index < events.len() {
        let event = &events[index];

        if in_delimiter_row {
            if event.kind == Kind::Enter {
                // Start of alignment value: set a new column.
                if event.name == Name::GfmTableDelimiterCellValue {
                    align.push(if events[index + 1].name == Name::GfmTableDelimiterMarker {
                        AlignKind::Left
                    } else {
                        AlignKind::None
                    });
                }
            } else {
                // End of alignment value: change the column.
                if event.name == Name::GfmTableDelimiterCellValue {
                    if events[index - 1].name == Name::GfmTableDelimiterMarker {
                        let align_index = align.len() - 1;
                        align[align_index] = if align[align_index] == AlignKind::Left {
                            AlignKind::Center
                        } else {
                            AlignKind::Right
                        }
                    }
                }
                // Done!
                else if event.name == Name::GfmTableDelimiterRow {
                    break;
                }
            }
        } else if event.kind == Kind::Enter && event.name == Name::GfmTableDelimiterRow {
            in_delimiter_row = true;
        }

        index += 1;
    }

    align
}
