//! pulldown-cmark's events normalized to follow the document's structure, before the
//! body walk reads them (§4.6).

use std::ops::Range;

use pulldown_cmark::{Event, Tag, TagEnd};

/// The events of `text` as the document's structure has them, where pulldown-cmark's
/// differ. Task-list boxes are dropped: pulldown-cmark reports one after its item's
/// first event, even inside a heading that follows it, and loci reads the box as part
/// of the item's marker. Then [`cover_escapes`] and [`tight_paragraphs`].
pub(super) fn normalize<'a>(
    text: &str,
    events: impl Iterator<Item = (Event<'a>, Range<usize>)>,
) -> Vec<(Event<'a>, Range<usize>)> {
    let events = events.filter(|(e, _)| !matches!(e, Event::TaskListMarker(_)));
    tight_paragraphs(cover_escapes(text, events))
}

/// The events with each text event that starts with an escaped character (`\#`)
/// covering its backslash, which pulldown-cmark leaves out of the range: what starts at
/// that text (an item's lead, a heading's content) starts at the backslash.
fn cover_escapes<'a>(
    text: &str,
    events: impl Iterator<Item = (Event<'a>, Range<usize>)>,
) -> impl Iterator<Item = (Event<'a>, Range<usize>)> {
    // The end of the last event that is not a tag: a backslash before it is its own.
    let mut covered = 0;
    events.map(move |(event, mut range)| {
        if let Event::Text(t) = &event {
            let escaped = t.starts_with(|c: char| c.is_ascii_punctuation())
                && covered < range.start
                && text[..range.start].ends_with('\\');
            if escaped {
                range.start -= 1;
            }
        }
        if !matches!(event, Event::Start(_) | Event::End(_)) {
            covered = range.end;
        }
        (event, range)
    })
}

/// The events with each run of inline events directly in a list item wrapped in a
/// `Paragraph` spanning the run, which pulldown-cmark leaves out in a tight list, so a
/// tight item reads as a loose one.
fn tight_paragraphs<'a>(events: impl Iterator<Item = (Event<'a>, Range<usize>)>) -> Vec<(Event<'a>, Range<usize>)> {
    let mut out = vec![];
    let mut run: Vec<(Event<'a>, Range<usize>)> = vec![];
    // For each open tag, whether it is an item; inline tags are not on it.
    let mut open_is_item: Vec<bool> = vec![];
    for (event, range) in events {
        let inline = match &event {
            Event::Start(tag) => is_inline(&tag.to_end()),
            Event::End(tag) => is_inline(tag),
            Event::Rule => false,
            _ => true,
        };
        if inline && open_is_item.last() == Some(&true) {
            run.push((event, range));
            continue;
        }
        if let Some((_, first)) = run.first() {
            let span = first.start..run.iter().map(|(_, r)| r.end).fold(first.end, usize::max);
            out.push((Event::Start(Tag::Paragraph), span.clone()));
            out.append(&mut run);
            out.push((Event::End(TagEnd::Paragraph), span));
        }
        match &event {
            Event::Start(tag) if !inline => open_is_item.push(matches!(tag, Tag::Item)),
            Event::End(tag) if !inline => {
                open_is_item.pop();
            }
            _ => {}
        }
        out.push((event, range));
    }
    out
}

/// Whether a tag is inline markup rather than a block.
fn is_inline(tag: &TagEnd) -> bool {
    matches!(
        tag,
        TagEnd::Emphasis
            | TagEnd::Strong
            | TagEnd::Strikethrough
            | TagEnd::Superscript
            | TagEnd::Subscript
            | TagEnd::Link
            | TagEnd::Image
    )
}
