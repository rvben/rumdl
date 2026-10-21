//! The document content type.
//!
//! **Document** represents the containers, such as block quotes, list items,
//! or GFM footnotes, which structure the document and contain other sections.
//!
//! The constructs found in flow are:
//!
//! * [Block quote][crate::mdx_parser::construct::block_quote]
//! * [List item][crate::mdx_parser::construct::list_item]
//! * [GFM: Footnote definition][crate::mdx_parser::construct::gfm_footnote_definition]

use crate::mdx_parser::event::{Content, Event, Kind, Link, Name};
use crate::mdx_parser::message;
use crate::mdx_parser::state::{Name as StateName, State};
use crate::mdx_parser::subtokenize::divide_events;
use crate::mdx_parser::tokenizer::{Container, ContainerState, Tokenizer};
use crate::mdx_parser::util::skip;
use alloc::{boxed::Box, vec::Vec};

/// Phases where we can exit containers.
#[derive(Debug, PartialEq)]
enum Phase {
    /// After parsing a line of lazy flow which resulted in something that
    /// exits containers before the line.
    ///
    /// ```markdown
    ///   | * a
    /// > | ```js
    ///          ^
    ///   | b
    ///   | ```
    /// ```
    After,
    /// When a new container replaces an existing container.
    ///
    /// ```markdown
    ///   | * a
    /// > | > b
    ///     ^
    /// ```
    Prefix,
    /// After everything.
    ///
    /// ```markdown
    /// > | * a
    ///        ^
    /// ```
    Eof,
}

/// Start of document, at an optional BOM.
///
/// ```markdown
/// > | a
///     ^
/// ```
pub fn start(tokenizer: &mut Tokenizer) -> State {
    tokenizer.tokenize_state.document_child =
        Some(Box::new(Tokenizer::new(tokenizer.point.clone(), tokenizer.parse_state)));

    tokenizer.attempt(
        State::Next(StateName::DocumentBeforeFrontmatter),
        State::Next(StateName::DocumentBeforeFrontmatter),
    );

    State::Retry(StateName::BomStart)
}

/// At optional frontmatter.
///
/// ```markdown
/// > | ---
///     ^
///   | title: Venus
///   | ---
/// ```
pub fn before_frontmatter(tokenizer: &mut Tokenizer) -> State {
    tokenizer.attempt(
        State::Next(StateName::DocumentContainerNewBefore),
        State::Next(StateName::DocumentContainerNewBefore),
    );
    State::Retry(StateName::FrontmatterStart)
}

/// At optional existing containers.
//
/// ```markdown
///   | * a
/// > | > b
///     ^
/// ```
pub fn container_existing_before(tokenizer: &mut Tokenizer) -> State {
    // On an empty line, skip containers that continued on the previous one.
    if tokenizer.current == Some(b'\n') {
        let state = &mut tokenizer.tokenize_state;

        if state.document_blank_from.is_none() {
            state.document_blank_from = Some(state.document_continued);
        }

        if state.document_continued >= state.document_blank_skip {
            debug_assert!(
                state.document_blank_skip <= state.document_container_stack.len(),
                "expected the stack to not change on an empty line"
            );
            state.document_continued = state.document_container_stack.len();
        }
    }

    // If there are more existing containers, check whether the next one continues.
    if tokenizer.tokenize_state.document_continued < tokenizer.tokenize_state.document_container_stack.len() {
        let container = &tokenizer.tokenize_state.document_container_stack[tokenizer.tokenize_state.document_continued];

        let name = match container.kind {
            Container::BlockQuote => StateName::BlockQuoteContStart,
            Container::GfmFootnoteDefinition => StateName::GfmFootnoteDefinitionContStart,
            Container::ListItem => StateName::ListItemContStart,
        };

        tokenizer.attempt(
            State::Next(StateName::DocumentContainerExistingAfter),
            State::Next(StateName::DocumentContainerNewBefore),
        );

        State::Retry(name)
    }
    // Otherwise, check new containers.
    else {
        State::Retry(StateName::DocumentContainerNewBefore)
    }
}

/// After continued existing container.
//
/// ```markdown
///   | * a
/// > |   b
///       ^
/// ```
pub fn container_existing_after(tokenizer: &mut Tokenizer) -> State {
    tokenizer.tokenize_state.document_continued += 1;
    State::Retry(StateName::DocumentContainerExistingBefore)
}

/// At new containers.
//
/// ```markdown
/// > | * a
///     ^
/// > | > b
///     ^
/// ```
pub fn container_new_before(tokenizer: &mut Tokenizer) -> State {
    // If we have completely continued, restore the flow’s past `interrupt`
    // status.
    if tokenizer.tokenize_state.document_continued == tokenizer.tokenize_state.document_container_stack.len() {
        let child = tokenizer.tokenize_state.document_child.as_ref().unwrap();

        tokenizer.interrupt = child.interrupt;

        // …and if we’re in a concrete construct, new containers can’t “pierce”
        // into them.
        if child.concrete {
            return State::Retry(StateName::DocumentContainersAfter);
        }
    }

    // Check for a new container.
    // Block quote?
    // Add a new container at the end of the stack.
    let tail = tokenizer.tokenize_state.document_container_stack.len();
    tokenizer.tokenize_state.document_container_stack.push(ContainerState {
        kind: Container::BlockQuote,
        blank_initial: false,
        size: 0,
    });
    // Swap the existing container with the new one.
    tokenizer
        .tokenize_state
        .document_container_stack
        .swap(tokenizer.tokenize_state.document_continued, tail);

    tokenizer.attempt(
        State::Next(StateName::DocumentContainerNewAfter),
        State::Next(StateName::DocumentContainerNewBeforeNotBlockQuote),
    );
    State::Retry(StateName::BlockQuoteStart)
}

/// At new container, but not a block quote.
//
/// ```markdown
/// > | * a
///     ^
/// ```
pub fn container_new_before_not_block_quote(tokenizer: &mut Tokenizer) -> State {
    // List item?
    // We replace the empty block quote container for this new list item one.
    tokenizer.tokenize_state.document_container_stack[tokenizer.tokenize_state.document_continued] = ContainerState {
        kind: Container::ListItem,
        blank_initial: false,
        size: 0,
    };

    tokenizer.attempt(
        State::Next(StateName::DocumentContainerNewAfter),
        State::Next(StateName::DocumentContainerNewBeforeNotList),
    );
    State::Retry(StateName::ListItemStart)
}

/// At new container, but not a block quote or list item.
//
/// ```markdown
/// > | a
///     ^
/// ```
pub fn container_new_before_not_list(tokenizer: &mut Tokenizer) -> State {
    // Footnote definition?
    // We replace the empty list item container for this new footnote
    // definition one.
    tokenizer.tokenize_state.document_container_stack[tokenizer.tokenize_state.document_continued] = ContainerState {
        kind: Container::GfmFootnoteDefinition,
        blank_initial: false,
        size: 0,
    };

    tokenizer.attempt(
        State::Next(StateName::DocumentContainerNewAfter),
        State::Next(StateName::DocumentContainerNewBeforeNotGfmFootnoteDefinition),
    );
    State::Retry(StateName::GfmFootnoteDefinitionStart)
}

/// At new container, but not a block quote, list item, or footnote definition.
//
/// ```markdown
/// > | a
///     ^
/// ```
pub fn container_new_before_not_footnote_definition(tokenizer: &mut Tokenizer) -> State {
    // It wasn’t a new block quote, list item, or footnote definition.
    // Swap the new container (in the middle) with the existing one (at the end).
    // Drop what was in the middle.
    tokenizer
        .tokenize_state
        .document_container_stack
        .swap_remove(tokenizer.tokenize_state.document_continued);

    State::Retry(StateName::DocumentContainersAfter)
}

/// After new container.
///
/// ```markdown
/// > | * a
///       ^
/// > | > b
///       ^
/// ```
pub fn container_new_after(tokenizer: &mut Tokenizer) -> State {
    // It was a new block quote, list item, or footnote definition.
    // Swap the new container (in the middle) with the existing one (at the end).
    // Take the new container.
    let container = tokenizer
        .tokenize_state
        .document_container_stack
        .swap_remove(tokenizer.tokenize_state.document_continued);

    // If we did not continue all existing containers, and there is a new one,
    // close the flow and those containers.
    if tokenizer.tokenize_state.document_continued != tokenizer.tokenize_state.document_container_stack.len() {
        if let Err(message) = exit_containers(tokenizer, &Phase::Prefix) {
            return State::error(message);
        }
    }

    // We are “piercing” into the flow with a new container.
    tokenizer.tokenize_state.document_child.as_mut().unwrap().pierce = true;

    tokenizer.tokenize_state.document_container_stack.push(container);
    tokenizer.tokenize_state.document_continued += 1;
    tokenizer.interrupt = false;
    State::Retry(StateName::DocumentContainerNewBefore)
}

/// After containers, at flow.
//
/// ```markdown
/// > | * a
///       ^
/// > | > b
///       ^
/// ```
pub fn containers_after(tokenizer: &mut Tokenizer) -> State {
    let child = tokenizer.tokenize_state.document_child.as_mut().unwrap();

    child.lazy = tokenizer.tokenize_state.document_continued != tokenizer.tokenize_state.document_container_stack.len();
    child.define_skip(tokenizer.point.clone());

    if tokenizer.current.is_none() {
        State::Retry(StateName::DocumentFlowEnd)
    } else {
        let current = tokenizer.events.len();
        let previous = tokenizer.tokenize_state.document_data_index;
        if let Some(previous) = previous {
            tokenizer.events[previous].link.as_mut().unwrap().next = Some(current);
        }
        tokenizer.tokenize_state.document_data_index = Some(current);
        tokenizer.enter_link(
            Name::Data,
            Link {
                previous,
                next: None,
                content: Content::Flow,
            },
        );
        State::Retry(StateName::DocumentFlowInside)
    }
}

/// In flow.
//
/// ```markdown
/// > | * ab
///       ^
/// ```
pub fn flow_inside(tokenizer: &mut Tokenizer) -> State {
    match tokenizer.current {
        None => {
            tokenizer.exit(Name::Data);
            State::Retry(StateName::DocumentFlowEnd)
        }
        // Note: EOL is part of data.
        Some(b'\n') => {
            tokenizer.consume();
            tokenizer.exit(Name::Data);
            State::Next(StateName::DocumentFlowEnd)
        }
        Some(_) => {
            tokenizer.consume_line_run();
            State::Next(StateName::DocumentFlowInside)
        }
    }
}

/// After flow (after eol or at eof).
//
/// ```markdown
///   | * a
/// > | > b
///     ^  ^
/// ```
pub fn flow_end(tokenizer: &mut Tokenizer) -> State {
    // Only when all containers continued.
    let state = &mut tokenizer.tokenize_state;
    state.document_blank_skip = match state.document_blank_from.take() {
        Some(from) if state.document_continued == state.document_container_stack.len() => from,
        _ => usize::MAX,
    };

    let child = tokenizer.tokenize_state.document_child.as_mut().unwrap();
    let state = tokenizer
        .tokenize_state
        .document_child_state
        .take()
        .unwrap_or(State::Next(StateName::FlowStart));

    tokenizer.tokenize_state.document_exits.push(None);

    let state = child.push(
        (child.point.index, child.point.vs),
        (tokenizer.point.index, tokenizer.point.vs),
        state,
    );

    tokenizer.tokenize_state.document_child_state = Some(state);

    // If we’re in a lazy line, and the previous (lazy or not) line is something
    // that can be lazy, and this line is that too, allow it.
    //
    // Accept:
    //
    // ```markdown
    //   | * a
    // > | b
    //     ^
    //   | ```
    // ```
    //
    // Do not accept:
    //
    // ```markdown
    //   | * # a
    // > | b
    //     ^
    //   | ```
    // ```
    //
    // Do not accept:
    //
    // ```markdown
    //   | * a
    // > | # b
    //     ^
    //   | ```
    // ```
    let mut document_lazy_continuation_current = false;
    let mut stack_index = child.stack.len();

    // Use two algo’s: one for when we’re suspended or in multiline things
    // like definitions, another for when we fed the line ending and closed.
    while !document_lazy_continuation_current && stack_index > 0 {
        stack_index -= 1;
        let name = &child.stack[stack_index];
        if name == &Name::Content || name == &Name::GfmTableHead {
            document_lazy_continuation_current = true;
        }
    }

    // …another because we parse each “rest” line as a paragraph, and we passed
    // a EOL already.
    if !document_lazy_continuation_current && !child.events.is_empty() {
        let before = skip::opt_back(&child.events, child.events.len() - 1, &[Name::LineEnding]);
        let name = &child.events[before].name;
        if name == &Name::Content || name == &Name::HeadingSetextUnderline {
            document_lazy_continuation_current = true;
        }
    }

    // Reset “piercing”.
    child.pierce = false;

    if child.lazy && tokenizer.tokenize_state.document_lazy_accepting_before && document_lazy_continuation_current {
        tokenizer.tokenize_state.document_continued = tokenizer.tokenize_state.document_container_stack.len();
    }

    if tokenizer.tokenize_state.document_continued != tokenizer.tokenize_state.document_container_stack.len() {
        let result = exit_containers(tokenizer, &Phase::After);
        // `Phase::After` doesn’t deal with flow: it only generates exits for
        // containers.
        // And that never errors.
        debug_assert!(result.is_ok(), "did not expect error when exiting");
    }

    if tokenizer.current.is_none() {
        tokenizer.tokenize_state.document_continued = 0;
        if let Err(message) = exit_containers(tokenizer, &Phase::Eof) {
            return State::error(message);
        }
        resolve(tokenizer);
        State::Ok
    } else {
        tokenizer.tokenize_state.document_continued = 0;
        tokenizer.tokenize_state.document_lazy_accepting_before = document_lazy_continuation_current;
        // Containers would only be interrupting if we’ve continued.
        tokenizer.interrupt = false;
        State::Retry(StateName::DocumentContainerExistingBefore)
    }
}

/// Close containers (and flow if needed).
fn exit_containers(tokenizer: &mut Tokenizer, phase: &Phase) -> Result<(), message::Message> {
    let mut stack_close = tokenizer
        .tokenize_state
        .document_container_stack
        .split_off(tokenizer.tokenize_state.document_continued);

    let child = tokenizer.tokenize_state.document_child.as_mut().unwrap();

    // Flush if needed.
    if *phase != Phase::After {
        let state = tokenizer
            .tokenize_state
            .document_child_state
            .take()
            .unwrap_or(State::Next(StateName::FlowStart));

        child.flush(state, false)?;
    }

    if !stack_close.is_empty() {
        let index = tokenizer.tokenize_state.document_exits.len() - (if *phase == Phase::After { 2 } else { 1 });
        let mut exits = Vec::with_capacity(stack_close.len());

        while let Some(container) = stack_close.pop() {
            let name = match container.kind {
                Container::BlockQuote => Name::BlockQuote,
                Container::GfmFootnoteDefinition => Name::GfmFootnoteDefinition,
                Container::ListItem => Name::ListItem,
            };

            exits.push(Event {
                kind: Kind::Exit,
                name: name.clone(),
                point: tokenizer.point.clone(),
                link: None,
            });

            let mut stack_index = tokenizer.stack.len();
            let mut found = false;

            while stack_index > 0 {
                stack_index -= 1;

                if tokenizer.stack[stack_index] == name {
                    tokenizer.stack.remove(stack_index);
                    found = true;
                    break;
                }
            }

            debug_assert!(found, "expected to find container event to exit");
        }

        debug_assert!(
            tokenizer.tokenize_state.document_exits[index].is_none(),
            "expected no exits yet"
        );
        tokenizer.tokenize_state.document_exits[index] = Some(exits);
    }

    child.interrupt = false;

    Ok(())
}

// Inject everything together.
fn resolve(tokenizer: &mut Tokenizer) {
    let child = tokenizer.tokenize_state.document_child.as_mut().unwrap();

    // First, add the container exits into `child`.
    let mut child_index = 0;
    let mut line = 0;

    while child_index < child.events.len() {
        if child.events[child_index].kind == Kind::Exit
            && matches!(child.events[child_index].name, Name::LineEnding | Name::BlankLineEnding)
        {
            // Inject before `Enter:LineEnding`.
            let mut inject_index = child_index - 1;
            let mut point = &child.events[inject_index].point;

            while child_index + 1 < child.events.len() && child.events[child_index + 1].kind == Kind::Exit {
                child_index += 1;
                point = &child.events[child_index].point;
                // Inject after `Exit:*`.
                inject_index = child_index + 1;
            }

            if line < tokenizer.tokenize_state.document_exits.len() {
                if let Some(mut exits) = tokenizer.tokenize_state.document_exits[line].take() {
                    let mut exit_index = 0;
                    while exit_index < exits.len() {
                        exits[exit_index].point = point.clone();
                        exit_index += 1;
                    }

                    child.map.add(inject_index, 0, exits);
                }
            }

            line += 1;
        }

        child_index += 1;
    }

    child.map.consume(&mut child.events);

    let mut flow_index = skip::to(&tokenizer.events, 0, &[Name::Data]);
    while flow_index < tokenizer.events.len()
        // To do: use `!is_some_and()` when that’s stable.
        && (tokenizer.events[flow_index].link.is_none()
            || tokenizer.events[flow_index].link.as_ref().unwrap().content != Content::Flow)
    {
        flow_index = skip::to(&tokenizer.events, flow_index + 1, &[Name::Data]);
    }

    // Now, add all child events into our parent document tokenizer.
    divide_events(
        &mut tokenizer.map,
        &tokenizer.events,
        flow_index,
        &mut child.events,
        (0, 0),
    );

    // Replace the flow data with actual events.
    tokenizer.map.consume(&mut tokenizer.events);

    // Now, add some final container exits due to the EOF.
    // We can’t inject them into the child earlier, as they are “outside” its
    // linked data.
    if line < tokenizer.tokenize_state.document_exits.len() {
        if let Some(mut exits) = tokenizer.tokenize_state.document_exits[line].take() {
            let mut exit_index = 0;
            while exit_index < exits.len() {
                exits[exit_index].point = tokenizer.point.clone();
                exit_index += 1;
            }

            tokenizer.events.append(&mut exits);
        }
    }

    // Add the resolvers from child.
    tokenizer.resolvers.append(&mut child.resolvers.split_off(0));

    tokenizer
        .tokenize_state
        .definitions
        .append(&mut child.tokenize_state.definitions.split_off(0));
}
