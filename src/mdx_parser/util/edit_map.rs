//! Deal with several changes in events, batching them together.
//!
//! Preferably, changes should be kept to a minimum.
//! Sometimes, it’s needed to change the list of events, because parsing can be
//! messy, and it helps to expose a cleaner interface of events to the compiler
//! and other users.
//! It can also help to merge many adjacent similar events.
//! And, in other cases, it’s needed to parse subcontent: pass some events
//! through another tokenizer and inject the result.

use crate::mdx_parser::event::Event;
use alloc::{vec, vec::Vec};

/// Shift `previous` and `next` links according to `jumps`.
///
/// This fixes links in case there are events removed or added between them.
fn shift_links(events: &mut [Event], jumps: &[(usize, usize, usize)]) {
    let mut jump_index = 0;
    let mut index = 0;
    let mut add = 0;
    let mut rm = 0;

    while index < events.len() {
        let rm_curr = rm;

        while jump_index < jumps.len() && jumps[jump_index].0 <= index {
            add = jumps[jump_index].2;
            rm = jumps[jump_index].1;
            jump_index += 1;
        }

        // Ignore items that will be removed.
        if rm > rm_curr {
            index += rm - rm_curr;
        } else {
            if let Some(link) = &events[index].link {
                if let Some(next) = link.next {
                    events[next].link.as_mut().unwrap().previous = Some(index + add - rm);

                    while jump_index < jumps.len() && jumps[jump_index].0 <= next {
                        add = jumps[jump_index].2;
                        rm = jumps[jump_index].1;
                        jump_index += 1;
                    }

                    events[index].link.as_mut().unwrap().next = Some(next + add - rm);
                    index = next;
                    continue;
                }
            }

            index += 1;
        }
    }
}

/// One requested change: remove `remove` events at `at`, then insert `add`.
#[derive(Debug)]
struct Edit {
    at: usize,
    remove: usize,
    add: Vec<Event>,
    /// Whether `add` goes before additions made earlier at the same index.
    before: bool,
    /// Call order, which orders the additions at one index.
    sequence: usize,
}

/// Tracks a bunch of edits.
///
/// Edits are logged as they come and applied in one pass, in `consume`.
#[derive(Debug)]
pub struct EditMap {
    /// Record of changes.
    edits: Vec<Edit>,
}

impl EditMap {
    /// Create a new edit map.
    pub fn new() -> EditMap {
        EditMap { edits: vec![] }
    }
    /// Create an edit: a remove and/or add at a certain place.
    pub fn add(&mut self, index: usize, remove: usize, add: Vec<Event>) {
        self.log(index, remove, add, false);
    }
    /// Create an edit: but insert `add` before existing additions.
    pub fn add_before(&mut self, index: usize, remove: usize, add: Vec<Event>) {
        self.log(index, remove, add, true);
    }
    /// Record an edit.
    fn log(&mut self, at: usize, remove: usize, add: Vec<Event>, before: bool) {
        if remove == 0 && add.is_empty() {
            return;
        }

        let sequence = self.edits.len();
        self.edits.push(Edit {
            at,
            remove,
            add,
            before,
            sequence,
        });
    }
    /// Done, change the events.
    pub fn consume(&mut self, events: &mut Vec<Event>) {
        if self.edits.is_empty() {
            return;
        }

        self.edits.sort_unstable_by_key(|edit| (edit.at, edit.sequence));

        // Calculate jumps: where items in the current list move to.
        let mut jumps = Vec::with_capacity(self.edits.len());
        let mut add_acc = 0;
        let mut remove_acc = 0;
        let mut index = 0;
        while index < self.edits.len() {
            let edit = &self.edits[index];
            remove_acc += edit.remove;
            add_acc += edit.add.len();
            index += 1;
            if index == self.edits.len() || self.edits[index].at != edit.at {
                jumps.push((edit.at, remove_acc, add_acc));
            }
        }

        shift_links(events, &jumps);

        let len = events.len();
        let old = core::mem::replace(events, Vec::with_capacity(len + add_acc - remove_acc));
        let mut old = old.into_iter();
        let mut cursor = 0;
        let mut start = 0;
        while start < self.edits.len() {
            let at = self.edits[start].at;
            let mut end = start + 1;
            while end < self.edits.len() && self.edits[end].at == at {
                end += 1;
            }
            let group = &mut self.edits[start..end];

            assert!(at >= cursor, "expected edits to not overlap");
            events.extend(old.by_ref().take(at - cursor));

            let mut remove = 0;
            for edit in group.iter_mut().rev() {
                if edit.before {
                    events.append(&mut edit.add);
                }
            }
            for edit in group.iter_mut() {
                remove += edit.remove;
                if !edit.before {
                    events.append(&mut edit.add);
                }
            }

            old.by_ref().take(remove).for_each(drop);
            cursor = at + remove;
            start = end;
        }
        assert!(cursor <= len, "expected edits to stay within events");
        events.extend(old);

        self.edits.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mdx_parser::event::{Content, Kind, Link, Name, Point};
    use core::convert::TryFrom;

    /// An event identified by `tag`, stored in its point index.
    fn event(tag: usize) -> Event {
        Event {
            kind: Kind::Enter,
            name: Name::Data,
            point: Point {
                line: 1,
                column: 1,
                index: tag,
                vs: 0,
            },
            link: None,
        }
    }

    fn events(tags: &[usize]) -> Vec<Event> {
        tags.iter().map(|tag| event(*tag)).collect()
    }

    fn tags(events: &[Event]) -> Vec<usize> {
        events.iter().map(|event| event.point.index).collect()
    }

    /// Reference model: merges calls per index, then splices.
    fn reference(calls: &[(usize, usize, Vec<Event>, bool)], events: &mut Vec<Event>) {
        let mut map: Vec<(usize, usize, Vec<Event>)> = vec![];
        for (at, remove, add, before) in calls {
            let mut add = add.clone();
            if *remove == 0 && add.is_empty() {
                continue;
            }
            if let Some(entry) = map.iter_mut().find(|entry| entry.0 == *at) {
                entry.1 += remove;
                if *before {
                    add.append(&mut entry.2);
                    entry.2 = add;
                } else {
                    entry.2.append(&mut add);
                }
            } else {
                map.push((*at, *remove, add));
            }
        }
        map.sort_unstable_by_key(|entry| entry.0);
        let mut result = vec![];
        let mut cursor = 0;
        for (at, remove, add) in map {
            result.extend(events[cursor..at].iter().cloned());
            result.extend(add);
            cursor = at + remove;
        }
        result.extend(events[cursor..].iter().cloned());
        *events = result;
    }

    #[test]
    fn test_edit_map_order() {
        let mut list = events(&[0, 1, 2]);
        let mut map = EditMap::new();
        map.add(1, 0, events(&[10]));
        map.add_before(1, 0, events(&[20]));
        map.add(1, 0, events(&[11]));
        map.add_before(1, 0, events(&[21]));
        map.consume(&mut list);
        assert_eq!(
            tags(&list),
            vec![0, 21, 20, 10, 11, 1, 2],
            "should put `add_before` chunks in reverse call order before `add` chunks in call order"
        );

        let mut list = events(&[0, 1, 2, 3]);
        map.add(3, 0, events(&[30]));
        map.add(1, 1, events(&[10]));
        map.add(1, 1, vec![]);
        map.add(2, 0, vec![]);
        map.consume(&mut list);
        assert_eq!(
            tags(&list),
            vec![0, 10, 30, 3],
            "should apply edits by index, sum removes at one index, and ignore empty edits"
        );
    }

    #[test]
    fn test_edit_map_links() {
        let mut list = events(&[0, 1, 2]);
        list[0].link = Some(Link {
            previous: None,
            next: Some(2),
            content: Content::Text,
        });
        list[2].link = Some(Link {
            previous: Some(0),
            next: None,
            content: Content::Text,
        });
        let mut map = EditMap::new();
        map.add(1, 0, events(&[10, 11]));
        map.add_before(1, 0, events(&[9]));
        map.consume(&mut list);
        assert_eq!(tags(&list), vec![0, 9, 10, 11, 1, 2]);
        assert_eq!(
            (
                list[0].link.as_ref().unwrap().next,
                list[5].link.as_ref().unwrap().previous
            ),
            (Some(5), Some(0)),
            "should shift links across all events inserted at one index"
        );
    }

    #[test]
    fn test_edit_map_matches_reference() {
        let mut seed: u64 = 0x2545_f491_4f6c_dd1d;
        let mut next = |bound: usize| {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            usize::try_from(seed % u64::try_from(bound).unwrap()).unwrap()
        };

        for round in 0..500 {
            let length = 1 + next(48);
            let mut tag = 100;
            // Removes add up at one index, so one remove per index keeps edits apart.
            let mut removed = vec![false; length + 1];
            let calls: Vec<(usize, usize, Vec<Event>, bool)> = (0..next(80))
                .map(|_| {
                    let at = next(length + 1);
                    let remove = usize::from(at < length && !removed[at] && next(3) == 0);
                    removed[at] = removed[at] || remove == 1;
                    let add = (0..next(3))
                        .map(|_| {
                            tag += 1;
                            event(tag)
                        })
                        .collect();
                    (at, remove, add, next(2) == 0)
                })
                .collect();

            let mut expected: Vec<Event> = events(&(0..length).collect::<Vec<_>>());
            let mut actual = expected.clone();
            reference(&calls, &mut expected);
            let mut map = EditMap::new();
            for (at, remove, add, before) in calls {
                if before {
                    map.add_before(at, remove, add);
                } else {
                    map.add(at, remove, add);
                }
            }
            map.consume(&mut actual);
            assert_eq!(tags(&actual), tags(&expected), "round {}", round);
        }
    }
}
