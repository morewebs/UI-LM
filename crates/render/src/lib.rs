//! Draws a page as one HTML document a person can open in a browser.
//!
//! The recorder produces a page as JSON, and until this existed nothing drew it. This is
//! the other half of the loop the project is built around: the recorder observes a screen
//! and describes it, and something else renders that description. Rendering the page back
//! as text proves nothing was lost; rendering it as HTML proves the description is
//! *enough* — that a consumer which never saw the terminal can put the same content on a
//! screen of its own.
//!
//! It is deliberately plain: a black page, a monospace grid, borders where the application
//! drew borders, and the colours the recorded cells carried. No interaction, no layout
//! engine, no framework — one self-contained file with its styles inline, so opening it
//! needs nothing installed.
//!
//! What it does not do: it does not recreate the terminal's exact geometry. Lines are laid
//! out as text, so a page drawn at 160 columns reflows with the window. A future renderer
//! that cares about column alignment would use the `rows` and per-span offsets every
//! component already carries; this one is a readable proof, not a terminal emulator.

mod terminal;
pub use terminal::{render_terminal, round_trip, RoundTrip};

use std::fmt::Write as _;

use ui_lm_pages::{Body, Component, Envelope, InlineSpan};

/// Renders every page in `pages` into one document, in order.
pub fn render_pages(pages: &[Envelope], title: &str) -> String {
    let mut body = String::new();
    for (index, page) in pages.iter().enumerate() {
        let _ = write!(
            body,
            "<section class=\"page\" data-frame=\"{index}\">\n<header>frame {index}</header>\n"
        );
        for component in &page.create_surface.components {
            render_component(&mut body, component);
        }
        body.push_str("</section>\n");
    }
    document(&body, title)
}

/// Renders one page into its own document.
pub fn render_page(page: &Envelope, title: &str) -> String {
    let mut body = String::new();
    for component in &page.create_surface.components {
        render_component(&mut body, component);
    }
    document(&body, title)
}

/// Wraps rendered components in a document with its styles inline.
fn document(body: &str, title: &str) -> String {
    format!(
        "<!doctype html>
<html lang=\"en\">
<head>
<meta charset=\"utf-8\">
<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">
<title>{title}</title>
<style>
:root {{ color-scheme: dark; }}
body {{ margin: 0; padding: 24px; background: #0b0b0c; color: #d6d6d7;
        font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
        font-size: 13px; line-height: 1.45; }}
section.page {{ margin: 0 auto 28px; max-width: 1100px; }}
section.page > header {{ color: #6f6f76; font-size: 11px; letter-spacing: .08em;
        text-transform: uppercase; margin-bottom: 8px; }}
.line {{ white-space: pre; overflow-x: auto; }}
.block {{ margin: 6px 0; }}
.panel, .composer {{ border: 1px solid #33333a; border-radius: 10px; padding: 8px 10px;
        margin: 8px 0; background: #101013; }}
.composer {{ border-color: #4a4a55; }}
.panel > .tag, .composer > .tag {{ color: #6f6f76; font-size: 10px; letter-spacing: .1em;
        text-transform: uppercase; display: block; margin-bottom: 4px; }}
.tool {{ display: flex; gap: 8px; align-items: baseline; margin: 4px 0; }}
.tool .name {{ color: #8ab4f8; }}
.tool .target {{ color: #d6d6d7; }}
.tool .counts {{ color: #6f6f76; }}
.thought {{ color: #8f8f98; font-style: italic; margin: 4px 0; }}
.meter {{ display: flex; align-items: center; gap: 10px; margin: 6px 0; }}
.meter .track {{ flex: 1; height: 6px; background: #232329; border-radius: 3px;
        overflow: hidden; }}
.meter .fill {{ height: 100%; background: #4f9d69; }}
.diff .add {{ color: #7cc48a; }}
.diff .remove {{ color: #d87373; }}
.diff .context {{ color: #9a9aa2; }}
.tabs {{ color: #6f6f76; margin-bottom: 6px; }}
button.action {{ display: flex; gap: 10px; align-items: baseline; width: 100%;
        text-align: left; background: #14141a; color: inherit; border: 1px solid #2a2a31;
        border-radius: 8px; padding: 5px 9px; margin: 3px 0; font: inherit; cursor: pointer; }}
button.action:hover {{ border-color: #4a4a55; background: #191922; }}
button.action .target {{ color: #8ab4f8; margin-left: auto; }}
.tabs span.active {{ color: #d6d6d7; }}
</style>
</head>
<body>
{body}</body>
</html>
",
        title = escape(title)
    )
}

/// Renders one component, appending to `out`.
fn render_component(out: &mut String, component: &Component) {
    match &component.body {
        Body::Column { .. } => {}
        Body::Block {
            role,
            lines,
            spans,
            tabs,
            shortcuts,
            properties,
            ..
        } => {
            let class = match role.as_str() {
                "panel" => "block panel",
                "composer" => "block composer",
                _ => "block",
            };
            let _ = writeln!(
                out,
                "<div class=\"{class}\" data-role=\"{}\">",
                escape(role)
            );
            if class.contains("panel") || class.contains("composer") {
                let _ = writeln!(out, "<span class=\"tag\">{}</span>", escape(role));
            }
            if !tabs.is_empty() {
                let names: Vec<String> = tabs
                    .iter()
                    .enumerate()
                    .map(|(index, tab)| {
                        let text = escape(tab);
                        if index == 0 {
                            format!("<span class=\"active\">{text}</span>")
                        } else {
                            format!("<span>{text}</span>")
                        }
                    })
                    .collect();
                let _ = writeln!(out, "<div class=\"tabs\">{}</div>", names.join("  ·  "));
            }
            for (index, line) in lines.iter().enumerate() {
                let line_spans = spans.get(index).map(Vec::as_slice).unwrap_or_default();
                let _ = writeln!(
                    out,
                    "<div class=\"line\">{}</div>",
                    styled(line, line_spans)
                );
            }
            for shortcut in shortcuts {
                // A shortcut is drawn as a button, not as a row: the page already says
                // which key does the thing, and a button is the one place a reader expects
                // to be able to press it. `data-binding` carries the key as the page spells
                // it; whatever serves the page decides what that means.
                let _ = writeln!(
                    out,
                    "<button class=\"action\" data-binding=\"{}\">\
                     <span class=\"name\">{}</span>\
                     <span class=\"target\">{}</span></button>",
                    escape(&shortcut.key),
                    escape(&shortcut.label),
                    escape(&shortcut.key)
                );
            }
            for property in properties {
                let _ = writeln!(
                    out,
                    "<div class=\"tool\"><span class=\"name\">{}</span>\
                     <span class=\"target\">{}</span></div>",
                    escape(&property.label),
                    escape(&property.value)
                );
            }
            out.push_str("</div>\n");
        }
        Body::ToolCall { calls, .. } => {
            for call in calls {
                let counts = match (call.added, call.removed) {
                    (Some(added), Some(removed)) => format!("+{added} −{removed}"),
                    _ => String::new(),
                };
                let _ = writeln!(
                    out,
                    "<div class=\"tool\"><span class=\"name\">{}</span>\
                     <span class=\"target\">{}</span>\
                     <span class=\"counts\">{}</span></div>",
                    escape(&call.tool),
                    escape(call.target.as_deref().unwrap_or_default()),
                    escape(&counts)
                );
            }
        }
        Body::Thought { text, seconds, .. } => {
            let when = seconds
                .map(|seconds| format!(" for {seconds:.1}s"))
                .unwrap_or_default();
            let _ = writeln!(
                out,
                "<div class=\"thought\">thought{when}</div>\n<div class=\"line\">{}</div>",
                escape(text.trim())
            );
        }
        Body::Meter {
            used,
            limit,
            fraction,
            label,
            ..
        } => {
            let share = fraction.unwrap_or(0.0).clamp(0.0, 1.0) * 100.0;
            let _ = writeln!(
                out,
                "<div class=\"meter\"><span>{}</span>\
                 <span class=\"track\"><span class=\"fill\" style=\"width:{share:.2}%\"></span></span>\
                 <span>{}</span><span class=\"counts\">{}</span></div>",
                escape(used),
                escape(limit),
                escape(label.as_deref().unwrap_or_default())
            );
        }
        Body::Diff { lines, .. } => {
            out.push_str("<div class=\"block diff\">\n");
            for line in lines {
                let _ = writeln!(
                    out,
                    "<div class=\"line {}\">{}</div>",
                    escape(&line.kind),
                    styled(&line.text, &line.spans)
                );
            }
            out.push_str("</div>\n");
        }
    }
}

/// A line of text with its styled runs wrapped in spans.
fn styled(line: &str, spans: &[InlineSpan]) -> String {
    let characters: Vec<char> = line.chars().collect();
    let mut out = String::new();
    let mut cursor = 0usize;
    for span in spans {
        let start = usize::from(span.start).min(characters.len());
        let end = usize::from(span.end).min(characters.len());
        if start >= end || start < cursor {
            continue;
        }
        if start > cursor {
            out.push_str(&escape(
                &characters[cursor..start].iter().collect::<String>(),
            ));
        }
        let text: String = characters[start..end].iter().collect();
        let _ = write!(
            out,
            "<span style=\"{}\">{}</span>",
            css(span),
            escape(&text)
        );
        cursor = end;
    }
    if cursor < characters.len() {
        out.push_str(&escape(&characters[cursor..].iter().collect::<String>()));
    }
    out
}

/// The CSS a span's styling becomes.
fn css(span: &InlineSpan) -> String {
    let mut rules = String::new();
    if span.style.bold {
        rules.push_str("font-weight:700;");
    }
    if span.style.italic {
        rules.push_str("font-style:italic;");
    }
    if span.style.underline {
        rules.push_str("text-decoration:underline;");
    }
    if let Some(colour) = span.style.fg.as_deref().and_then(colour) {
        let _ = write!(rules, "color:{colour};");
    }
    if let Some(colour) = span.style.bg.as_deref().and_then(colour) {
        let _ = write!(rules, "background:{colour};");
    }
    if span.style.reverse {
        rules.push_str("filter:invert(1);");
    }
    rules
}

/// A page's colour spelling turned into a CSS colour.
///
/// The page keeps colours as the terminal reported them — `indexed:N` for the 256-colour
/// palette, `rgb:R,G,B` for true colour. The first sixteen palette entries have standard
/// values and are used as they are; anything past them has no canonical meaning without
/// the terminal's own palette, so it is shown neutrally rather than guessed.
fn colour(text: &str) -> Option<String> {
    if let Some(rest) = text.strip_prefix("rgb:") {
        return Some(format!("rgb({rest})"));
    }
    let index: u8 = text.strip_prefix("indexed:")?.parse().ok()?;
    const PALETTE: [&str; 16] = [
        "#000000", "#cd3131", "#0dbc79", "#e5e510", "#2472c8", "#bc3fbc", "#11a8cd", "#e5e5e5",
        "#666666", "#f14c4c", "#23d18b", "#f5f543", "#3b8eea", "#d670d6", "#29b8db", "#ffffff",
    ];
    Some(PALETTE[usize::from(index.min(15))].to_owned())
}

/// Escapes the five characters that would otherwise be markup.
fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            other => out.push(other),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use ui_lm_frames::frame_from_chars;
    use ui_lm_pages::page_from_frame;

    /// A page for a screen drawn by `lines`.
    fn page_of(lines: &[&str]) -> Envelope {
        let chars: Vec<Vec<char>> = lines.iter().map(|line| line.chars().collect()).collect();
        let frame = frame_from_chars(0, u16::try_from(lines.len()).unwrap(), 60, &chars);
        page_from_frame(&frame, "test")
    }

    #[test]
    fn the_screen_text_reaches_the_document() {
        let html = render_page(&page_of(&["hello there", "", "second block"]), "t");
        assert!(html.contains("hello there"), "{html}");
        assert!(html.contains("second block"), "{html}");
    }

    #[test]
    fn markup_in_the_screen_cannot_break_the_document() {
        // A terminal can show `<script>` quite happily; the document must not run it.
        let html = render_page(&page_of(&["<script>alert(1)</script> & done"]), "t");
        assert!(!html.contains("<script>alert(1)</script>"), "{html}");
        assert!(html.contains("&lt;script&gt;"), "{html}");
        assert!(html.contains("&amp; done"), "{html}");
    }

    #[test]
    fn a_meter_draws_a_bar_of_the_right_width() {
        let html = render_page(&page_of(&["1.2K / 500K | [Dashboard]"]), "t");
        assert!(html.contains("class=\"meter\""), "{html}");
        assert!(
            html.contains("width:0.24%"),
            "1.2K of 500K is 0.24%: {html}"
        );
    }

    #[test]
    fn a_diff_line_keeps_its_kind_as_a_class() {
        let html = render_page(&page_of(&["  - old", "  + new"]), "t");
        assert!(html.contains("class=\"line remove\""), "{html}");
        assert!(html.contains("class=\"line add\""), "{html}");
    }

    #[test]
    fn every_page_renders_into_one_document_in_order() {
        let pages = vec![page_of(&["first"]), page_of(&["second"])];
        let html = render_pages(&pages, "t");
        let first = html.find("first").unwrap();
        let second = html.find("second").unwrap();
        assert!(first < second, "pages must keep their order");
        assert_eq!(html.matches("class=\"page\"").count(), 2);
    }

    #[test]
    fn a_shortcut_is_drawn_as_a_button_carrying_its_binding() {
        // The page says which key does the thing; the button carries exactly that spelling
        // so whatever serves the page can decide what it means.
        // The keys are right-aligned, the way the application draws them.
        let page = page_of(&[
            "│ Resume Session              Ctrl+R │",
            "│ Agent Dashboard         /dashboard │",
        ]);
        let html = render_page(&page, "t");
        assert!(html.contains("class=\"action\""), "{html}");
        assert!(html.contains("data-binding=\"Ctrl+R\""), "{html}");
        assert!(html.contains("data-binding=\"/dashboard\""), "{html}");
        assert!(html.contains("Resume Session"), "{html}");
    }
}
