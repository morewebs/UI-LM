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
        body_of(page, &mut body);
        body.push_str("</section>\n");
    }
    document(&body, title)
}

/// The body of a page, with panes drawn side by side.
///
/// The terminal renderer has drawn a split screen in two columns since the container landed; this one
/// still put the agent's meter above the shell's prompt, in one column, because nothing had ever shown
/// it a paned page. `check-html` compared words and passed — words reach the document either way.
fn body_of(page: &Envelope, out: &mut String) {
    let panes = &page.create_surface.panes;
    if panes.len() != 2 {
        for component in &page.create_surface.components {
            render_component(out, component);
        }
        return;
    }
    // The marker says how the container lays its children out, in the document rather than only in the
    // stylesheet. A check can read an attribute; it cannot read a cascade, and an assertion that parsed the CSS
    // was reverted in round 122 because no mutant could be made to fail it.
    out.push_str("<div class=\"panes\" data-layout=\"row\">");
    for group in panes {
        out.push_str("<div class=\"pane\">");
        for id in group {
            if let Some(component) = page
                .create_surface
                .components
                .iter()
                .find(|component| component.id == *id)
            {
                render_component(out, component);
            }
        }
        out.push_str("</div>");
    }
    out.push_str("</div>");
    // A paned document also carries the **screen**: the rows a terminal draws, with the column boundary in them. The
    // panes above give a GUI its two containers, and neither view can be derived from the other - a reader that sees only
    // the containers puts the panes one after another instead of side by side (rounds 74-75).
    out.push_str("<div class=\"screen\">");
    for line in crate::terminal::render_terminal(page).lines() {
        let _ = write!(out, "<div class=\"line\">{}</div>", escape(line));
    }
    out.push_str("</div>");
}
/// Renders one page into its own document.
pub fn render_page(page: &Envelope, title: &str) -> String {
    let mut body = String::new();
    body_of(page, &mut body);
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
.panes {{ display: grid; grid-template-columns: 1fr 1fr; gap: 1.5rem; align-items: start; }}
      .pane {{ min-width: 0; }}
      .tool {{ display: flex; gap: 8px; align-items: baseline; margin: 4px 0; }}
.tool .name {{ color: #8ab4f8; }}
.tool .target {{ color: #d6d6d7; }}
        .property {{ display: flex; gap: 8px; align-items: baseline; margin: 4px 0; }}
        .property .name {{ color: #8ab4f8; }}
        .property .value {{ color: #d6d6d7; }}
        /* A paned document carries the screen as well as the panes: the screen is what `read` turns back into a terminal,
           and a browser has the panes. It had no styles, so the live view drew both - the split screen twice (round 103). */
        .screen {{ display: none; }}
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
        // ADR 0008: a table's rows are line elements too, so a document holding one can still be turned back into the
        // screen it came from - which `check-html-to-tui` requires of every kind that carries text.
        Body::Table {
            columns,
            cells,
            lines,
            ..
        } => {
            out.push_str("<div class=\"block table\">");
            out.push_str(&format!(" data-columns=\"{}\"", columns.len()));
            out.push('>');
            // The row's **own text** is the line element, so a document holding a table can still be turned back into
            // the screen (`check-html-to-tui`); the cells ride along as data for a runtime that wants columns.
            for (row, line) in cells.iter().zip(lines) {
                out.push_str(&format!(
                    "<div class=\"line row\" data-cells=\"{}\">",
                    escape(&row.join("\u{1f}"))
                ));
                out.push_str(&escape(line));
                out.push_str("</div>");
            }
            out.push_str("</div>\n");
        }
        Body::Frame { title, lines, .. } => {
            // ADR 0006: a window drawn over the page. Its rows carry the box art, so the same span renderer
            // that draws a block draws this; the class says what it is and the title is readable without
            // decoding the border.
            out.push_str("<div class=\"component frame\"");
            out.push_str(&format!(" data-title=\"{}\"", escape(title)));
            out.push('>');
            for line in lines {
                out.push_str("<div class=\"line\">");
                out.push_str(&escape(line));
                out.push_str("</div>");
            }
            out.push_str("</div>");
        }
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
            // The page says a shortcut is a command a runtime can offer as a button, and the renderer's own test says the
            // same. It had been drawing them as ordinary lines, which is why `a_shortcut_is_drawn_as_a_button_carrying_its_binding`
            // failed once `looks_like_binding` gave it real bindings to draw (round 32).
            if !shortcuts.is_empty() {
                out.push_str("<div class=\"shortcuts\">");
                for shortcut in shortcuts {
                    let _ = write!(
                        out,
                        "<button class=\"action\" data-binding=\"{}\">{}</button>",
                        escape(&shortcut.key),
                        escape(&shortcut.label)
                    );
                }
                out.push_str("</div>");
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
                    "<div class=\"property\"><span class=\"name\">{}</span>\
                     <span class=\"value\">{}</span></div>",
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

/// Read a terminal screen back out of an HTML document.
///
/// This is the other half of the loop the project exists for: a page becomes a GUI, and a GUI becomes a page again. It grew
/// up inside `check-html-to-tui` as a regex over `<div class="line">`, which only understands markup this project wrote.
///
/// It now also reads markup it did not write. A `<table>` whose cells carry no `class` becomes aligned columns, which is
/// what a terminal would have drawn; anything else becomes one line per block element, with tags removed and entities
/// decoded.
pub fn lines_from_html(html: &str) -> Vec<String> {
    // A paned document holds its panes **and** the screen they make; the screen is the one that has the column boundary
    // in it, so it is what a reader wants (round 75).
    let screen = match html.find("<div class=\"screen\">") {
        Some(at) => crate::lines_of_lines(&html[at..]),
        None => Vec::new(),
    };
    if !screen.is_empty() {
        return screen;
    }
    let ours = crate::lines_of_lines(html);
    if !ours.is_empty() {
        return ours;
    }
    let table = crate::lines_of_table(html);
    if !table.is_empty() {
        return table;
    }
    // One pass first: it sees prose, lists, headings and a form together, in the order the page draws them. The two older
    // readers stay as a fallback until the merged one has run on every document in the corpus (round 71).
    let flow = crate::lines_of_flow(html);
    if !flow.is_empty() {
        return flow;
    }
    let blocks = crate::lines_of_blocks(html);
    if !blocks.is_empty() {
        return blocks;
    }
    crate::lines_of_forms(html)
}

/// The `<div class="line">` rows this project's renderer writes, in order, with their tags removed.
fn lines_of_lines(html: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = html;
    // `class="line"` or `class="line row"` - never `class="lines"`, which is the container the rows sit in.
    let mut found = rest
        .find("<div class=\"line\"")
        .map(|at| (at, 17))
        .or_else(|| rest.find("<div class=\"line ").map(|at| (at, 17)));
    while let Some(start) = found.map(|(at, _)| at) {
        let after = &rest[start..];
        let Some(open_end) = after.find('>') else {
            break;
        };
        let body = &after[open_end + 1..];
        let Some(close) = body.find("</div>") else {
            break;
        };
        out.push(text_of(&body[..close]).trim_end().to_owned());
        rest = &body[close + 6..];
        found = rest
            .find("<div class=\"line\"")
            .map(|at| (at, 17))
            .or_else(|| rest.find("<div class=\"line ").map(|at| (at, 17)));
    }
    out
}

/// Rows of a `<table>`, with the cells padded to the widest in each column - what a terminal can draw.
fn lines_of_table(html: &str) -> Vec<String> {
    let mut rows: Vec<Vec<String>> = Vec::new();
    let mut rest = html;
    while let Some(start) = rest.find("<tr") {
        let after = &rest[start..];
        let Some(open_end) = after.find('>') else {
            break;
        };
        let body = &after[open_end + 1..];
        let Some(close) = body.find("</tr>") else {
            break;
        };
        let row_html = &body[..close];
        let mut cells = Vec::new();
        let mut cell_rest = row_html;
        while let Some(cell_start) = cell_rest.find("<td").or_else(|| cell_rest.find("<th")) {
            let cell_after = &cell_rest[cell_start..];
            let Some(cell_open_end) = cell_after.find('>') else {
                break;
            };
            let cell_body = &cell_after[cell_open_end + 1..];
            let cell_close = cell_body.find("</td>").or_else(|| cell_body.find("</th>"));
            let Some(cell_close) = cell_close else { break };
            cells.push(text_of(&cell_body[..cell_close]).trim().to_owned());
            cell_rest = &cell_body[cell_close + 5..];
        }
        if !cells.is_empty() {
            rows.push(cells);
        }
        rest = &body[close + 5..];
    }
    if rows.is_empty() {
        return Vec::new();
    }
    let columns = rows.iter().map(Vec::len).max().unwrap_or(0);
    let mut widths = vec![0usize; columns];
    for row in &rows {
        for (index, cell) in row.iter().enumerate() {
            widths[index] = widths[index].max(cell.chars().count());
        }
    }
    rows.iter()
        .map(|row| {
            let mut line = String::new();
            for (index, width) in widths.iter().enumerate() {
                let cell = row.get(index).map(String::as_str).unwrap_or("");
                if index + 1 == columns {
                    line.push_str(cell);
                } else {
                    line.push_str(&format!("{cell:<width$}  ", width = width));
                }
            }
            line.trim_end().to_owned()
        })
        .collect()
}

/// A form: each control in the shape a terminal gives it - a label as itself, a field as a box, a button in brackets.
///
/// A `<form>` used to read as **nothing at all**, because the reader only knew elements that contain text and a field
/// contains none (round 68).
fn lines_of_forms(html: &str) -> Vec<String> {
    let mut found: Vec<(usize, String)> = Vec::new();
    let mut from = 0;
    while let Some(offset) = html[from..].find('<') {
        let start = from + offset;
        let Some(open_end) = html[start..].find('>').map(|at| start + at) else {
            break;
        };
        let tag = &html[start..open_end];
        let lowered = tag.to_lowercase();
        let line = if lowered.starts_with("<input") || lowered.starts_with("<textarea") {
            let value = attribute(tag, "value").unwrap_or_default();
            let placeholder = attribute(tag, "placeholder").unwrap_or_default();
            let shown = if !value.is_empty() {
                value
            } else {
                placeholder
            };
            Some(format!("[ {shown} ]"))
        } else if lowered.starts_with("<button") {
            let close = html[open_end..].find("</button").map(|at| open_end + at);
            let text = close
                .map(|at| text_of_collapsed(&html[open_end + 1..at]).trim().to_owned())
                .unwrap_or_default();
            Some(format!("[ {text} ]"))
        } else if lowered.starts_with("<label") {
            let close = html[open_end..].find("</label").map(|at| open_end + at);
            close.map(|at| text_of_collapsed(&html[open_end + 1..at]).trim().to_owned())
        } else {
            None
        };
        if let Some(line) = line {
            if !line.trim().is_empty() {
                found.push((start, line));
            }
        }
        from = open_end + 1;
    }
    found.sort_by_key(|(at, _)| *at);
    found.into_iter().map(|(_, line)| line).collect()
}

/// The value of an attribute in a tag, without its quotes.
fn attribute(tag: &str, name: &str) -> Option<String> {
    let at = tag.to_lowercase().find(&format!("{name}="))?;
    let rest = &tag[at + name.len() + 1..];
    let quote = rest.chars().next()?;
    if quote != '"' && quote != '\'' {
        return Some(
            rest.split_whitespace()
                .next()
                .unwrap_or_default()
                .to_owned(),
        );
    }
    let end = rest[1..].find(quote)?;
    Some(rest[1..1 + end].to_owned())
}

/// One line per block element - and the shapes a terminal gives them: a bullet for a list item, and a heading underlined
/// with `=` or `-`, which is how every text renderer since `more` has marked one (round 67).
///
/// It closes on the **element it opened** - `</p>`, `</li>` - rather than on the first `</` it meets, which is how
/// `First <b>point</b>.` came out as `First point` with its full stop cut off (round 61).
fn lines_of_blocks(html: &str) -> Vec<String> {
    let tags = ["h1", "h2", "h3", "li", "p", "pre", "div"];
    let mut found: Vec<(usize, &str, String)> = Vec::new();
    for name in tags {
        let open = format!("<{name}");
        let close = format!("</{name}");
        let mut from = 0;
        while let Some(offset) = html[from..].find(&open) {
            let start = from + offset;
            let Some(open_end) = html[start..].find('>').map(|at| start + at) else {
                break;
            };
            let Some(close_at) = html[open_end..].find(&close).map(|at| open_end + at) else {
                break;
            };
            let text = text_of_collapsed(&html[open_end + 1..close_at])
                .trim()
                .to_owned();
            if !text.is_empty() {
                found.push((start, name, text));
            }
            from = close_at + close.len();
        }
    }
    found.sort_by_key(|(at, _, _)| *at);
    let mut out = Vec::new();
    for (_, name, text) in found {
        match name {
            "li" => out.push(format!("- {text}")),
            "h1" | "h2" => {
                let rule = if name == "h1" { '=' } else { '-' };
                let width = text.chars().count();
                out.push(text);
                out.push(rule.to_string().repeat(width));
            }
            _ => out.push(text),
        }
    }
    out
}

/// The text of a fragment: tags removed, entities decoded, whitespace collapsed the way a screen collapses it.
fn text_of(fragment: &str) -> String {
    decode(fragment, false)
}

/// Block text: whitespace collapsed, the way a browser renders a paragraph.
fn text_of_collapsed(fragment: &str) -> String {
    decode(fragment, true)
}

fn decode(fragment: &str, collapse: bool) -> String {
    let mut out = String::new();
    let mut inside_tag = false;
    for c in fragment.chars() {
        match c {
            '<' => inside_tag = true,
            '>' => inside_tag = false,
            other if !inside_tag => out.push(other),
            _ => {}
        }
    }
    let decoded = out
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&amp;", "&");
    if collapse {
        decoded.split_whitespace().collect::<Vec<_>>().join(" ")
    } else {
        decoded
    }
}

/// Every element of a document in **one** pass, in the order it appears, each in the shape a terminal gives it.
///
/// This exists because the two readers it replaces - blocks, then forms - had a fallback between them, so a page holding
/// prose **and** a form lost the form: the second reader only ran when the first produced nothing (rounds 69-71).
fn lines_of_flow(html: &str) -> Vec<String> {
    let mut found: Vec<(usize, &'static str, String)> = Vec::new();
    for name in [
        "h1", "h2", "h3", "li", "p", "pre", "div", "label", "dt", "dd", "summary",
    ] {
        let open = format!("<{name}");
        let close = format!("</{name}>");
        let _unused_close = &close;
        let mut from = 0;
        while let Some(offset) = html[from..].find(&open) {
            let start = from + offset;
            // The tag name must end here: `<p` is a prefix of `<pre`, so a scan for one matched both and every code block
            // was emitted twice - once as a paragraph and once as itself (round 85).
            let after_name = html[start + 1 + name.len()..].chars().next();
            if !matches!(
                after_name,
                Some('>') | Some(' ') | Some('\t') | Some('\n') | Some('\r')
            ) {
                from = start + 1;
                continue;
            }
            let Some(open_end) = html[start..].find('>').map(|at| start + at) else {
                break;
            };
            // The close must belong to **this** element: a list inside a list has a `</li>` that is not the outer
            // one's, and closing there glued `one` and `one.a` into `- oneone.a` (round 88).
            let close_at = match close_of(html, open_end, name) {
                Some(at) => at,
                None => break,
            };
            let inner = &html[open_end + 1..close_at];
            // A `<div>` that wraps other blocks is a container, not a line. Read as one it glues the whole page into a
            // single row, and every element inside is then emitted again - which is what `example.com` did to the reader
            // the first time it met a real page (round 79).
            if name == "div"
                && [
                    "<h1", "<h2", "<h3", "<p", "<li", "<div", "<table", "<form", "<ul",
                ]
                .iter()
                .any(|tag| inner.contains(tag))
            {
                // Step past the open tag, not the close: everything inside still gets its own line.
                from = open_end + 1;
                continue;
            }
            // A list inside a list is its own items: taking the outer item's text up to the inner list keeps `one` and
            // `one.a` apart instead of gluing them into `oneone.a` (round 88).
            let own = if name == "li" {
                let cut = inner
                    .find("<ul")
                    .or_else(|| inner.find("<ol"))
                    .unwrap_or(inner.len());
                &inner[..cut]
            } else {
                inner
            };
            let prepared = normalize_inline(own);
            let text = text_of_collapsed(&prepared).trim().to_owned();
            if !text.is_empty() {
                let kind: &'static str = match name {
                    "li" => "li",
                    "h1" => "h1",
                    "h2" => "h2",
                    "dt" => "dt",
                    "dd" => "dd",
                    _ => "p",
                };
                if name == "pre" {
                    // A code block is lines, not a paragraph: its breaks and its indentation are the content. The
                    // paragraph branch owns every other element, and the tag boundary above keeps `<p` from matching
                    // `<pre`, so this subtree is emitted exactly once (round 85).
                    for line in inner.lines() {
                        let line = line.trim_end();
                        if !line.is_empty() {
                            found.push((start, "code", line.to_owned()));
                        }
                    }
                } else {
                    if name != "pre" && prepared.contains('\n') {
                        // A break inside a paragraph is a line on the screen.
                        for piece in prepared.split('\n') {
                            let piece = text_of_collapsed(piece).trim().to_owned();
                            if !piece.is_empty() {
                                found.push((start, kind, piece));
                            }
                        }
                    } else {
                        found.push((start, kind, text));
                    }
                }
            }
            // Past the open tag, so a list inside a list is visited as well (round 89).
            from = open_end + 1;
        }
    }
    // A field and a button hold no text, so they are found by their own scan and merged by position.
    let mut at = 0;
    while let Some(offset) = html[at..].find('<') {
        let start = at + offset;
        let Some(open_end) = html[start..].find('>').map(|x| start + x) else {
            break;
        };
        let tag = &html[start..open_end];
        let lowered = tag.to_lowercase();
        if lowered.starts_with("<input") || lowered.starts_with("<textarea") {
            let value = attribute(tag, "value").unwrap_or_default();
            let placeholder = attribute(tag, "placeholder").unwrap_or_default();
            let shown = if !value.is_empty() {
                value
            } else {
                placeholder
            };
            found.push((start, "field", format!("[ {shown} ]")));
        } else if lowered.starts_with("<button") {
            let text = html[open_end..]
                .find("</button")
                .map(|x| {
                    text_of_collapsed(&html[open_end + 1..open_end + x])
                        .trim()
                        .to_owned()
                })
                .unwrap_or_default();
            found.push((start, "button", format!("[ {text} ]")));
        }
        at = open_end + 1;
    }
    found.sort_by_key(|(at, _, _)| *at);
    let mut out = Vec::new();
    for (_, kind, text) in found {
        match kind {
            "li" => out.push(format!("- {text}")),
            // A definition list is a term and its meaning, and a terminal indents the meaning under the term.
            "dd" => out.push(format!("    {text}")),
            "h1" | "h2" => {
                let rule = if kind == "h1" { '=' } else { '-' };
                let width = text.chars().count();
                out.push(text);
                out.push(rule.to_string().repeat(width));
            }
            _ => out.push(text),
        }
    }
    out
}

/// Prepare the inside of an element before its text is taken: a `<br>` is a line break, and an image speaks through its
/// `alt` text. Both were dropped - `first<br>second` read as `firstsecond`, and a diagram read as nothing (round 87).
fn normalize_inline(inner: &str) -> String {
    let mut out = String::new();
    let mut rest = inner;
    while let Some(at) = rest.find('<') {
        out.push_str(&rest[..at]);
        let tail = &rest[at..];
        let Some(end) = tail.find('>') else {
            out.push_str(tail);
            return out;
        };
        let tag = &tail[..=end];
        let lowered = tag.to_lowercase();
        if lowered.starts_with("<br") {
            out.push('\n');
        } else if lowered.starts_with("<img") {
            if let Some(alt) = attribute(tag, "alt") {
                out.push_str(&alt);
            }
        }
        // A tag between two runs of text is a gap: `<span>Key</span><span>Value</span>` is two cells, and reading it
        // as `KeyValue` was the framework page's first defect (round 92).
        let next_char = rest_after(tail, end);
        if !lowered.starts_with("<br")
            && !out.ends_with(' ')
            && next_char.is_some_and(|c| !matches!(c, '.' | ',' | ';' | ':' | '!' | '?' | ')'))
        {
            out.push(' ');
        }
        rest = &tail[end + 1..];
    }
    out.push_str(rest);
    out
}

/// The index of the `</name>` that closes the element opened at `open_end`, counting nested opens of the same name.
fn close_of(html: &str, open_end: usize, name: &str) -> Option<usize> {
    let open = format!("<{name}");
    let close = format!("</{name}");
    let mut depth = 1usize;
    let mut at = open_end;
    while at < html.len() {
        let next_open = html[at..].find(&open).map(|x| at + x);
        let next_close = html[at..].find(&close).map(|x| at + x);
        let (Some(open_at), Some(close_at)) = (next_open, next_close) else {
            return next_close;
        };
        if open_at < close_at {
            depth += 1;
            at = open_at + open.len();
        } else {
            depth -= 1;
            if depth == 0 {
                return Some(close_at);
            }
            at = close_at + close.len();
        }
    }
    None
}

/// The character that follows a tag which ends at `end` inside `tail`.
fn rest_after(tail: &str, end: usize) -> Option<char> {
    tail[end + 1..].chars().next()
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
    fn a_table_written_by_another_tool_is_read_as_columns() {
        // The GUI -> TUI direction on markup this project did not write: a plain table, no classes, no data attributes.
        let html = "<html><body><table>\
                    <tr><th>Name</th><th>Mode</th></tr>\
                    <tr><td>hosts</td><td>-a----</td></tr>\
                    <tr><td>lmhosts.sam</td><td>-a----</td></tr>\
                    </table></body></html>";
        let lines = lines_from_html(html);
        assert_eq!(lines.len(), 3, "{lines:?}");
        assert_eq!(lines[0], "Name         Mode");
        assert_eq!(lines[1], "hosts        -a----");
        assert_eq!(lines[2], "lmhosts.sam  -a----");
    }

    #[test]
    fn a_list_inside_a_list_keeps_both_items() {
        // The scan used to step past the outer item's close, so the inner item was never visited: `one.a` was first glued
        // to `one`, then dropped (rounds 88-89).
        let html = "<ul><li>one<ul><li>one.a</li></ul></li><li>two</li></ul>";
        assert_eq!(lines_from_html(html), vec!["- one", "- one.a", "- two"]);
    }

    #[test]
    fn a_definition_list_indents_its_meanings() {
        // A `<dl>` came back as nothing at all: neither `dt` nor `dd` was in the scan list (round 86).
        let html = "<dl><dt>Term</dt><dd>Its definition.</dd></dl>";
        assert_eq!(lines_from_html(html), vec!["Term", "    Its definition."]);
    }

    #[test]
    fn a_code_block_keeps_its_lines_and_appears_once() {
        // `<p` is a prefix of `<pre`, so the paragraph scan matched every code block and emitted it twice - once glued
        // and once as itself (rounds 83-85).
        let html = "<div><h2>Example</h2><pre>def f():\n    return 1\nprint(f())</pre></div>";
        let lines = lines_from_html(html);
        assert_eq!(
            lines,
            vec![
                "Example",
                "-------",
                "def f():",
                "    return 1",
                "print(f())"
            ]
        );
    }

    #[test]
    fn a_wrapper_div_is_not_a_line() {
        // `example.com` reduced to its shell: the div holds blocks, so it is a container. Read as a block it glued the
        // page into one row and repeated every element inside (round 79).
        let html = "<body><div><h1>Example Domain</h1><p>This domain is for use.</p></div></body>";
        let lines = lines_from_html(html);
        assert_eq!(
            lines,
            vec![
                "Example Domain",
                "==============",
                "This domain is for use."
            ]
        );
    }

    #[test]
    fn one_pass_keeps_prose_and_a_form_in_order() {
        let html = "<h1>Sign up</h1><p>Fill this in.</p><form><label>Name</label>\
                    <input value=\"Jane\"><button>Save</button></form><p>Thanks.</p>";
        let lines = lines_of_flow(html);
        assert_eq!(
            lines,
            vec![
                "Sign up",
                "=======",
                "Fill this in.",
                "Name",
                "[ Jane ]",
                "[ Save ]",
                "Thanks."
            ],
            "{lines:?}"
        );
    }

    #[test]
    fn a_form_reads_as_a_form() {
        // A `<form>` used to read as nothing: only elements that contain text were known, and a field contains none.
        let html = "<form><label>Name</label><input type=\"text\" value=\"Jane\">\
                    <label>Age</label><input type=\"number\"><button>Save</button></form>";
        let lines = lines_from_html(html);
        assert_eq!(lines, vec!["Name", "[ Jane ]", "Age", "[  ]", "[ Save ]"]);
    }

    #[test]
    fn a_document_reads_the_way_a_terminal_draws_it() {
        // Bullets for list items and a rule under a heading, which is how every text renderer marks one.
        let html = "<h1>Notes</h1><p>Two things.</p><ul><li>one</li><li>two</li></ul><h2>More</h2>";
        let lines = lines_from_html(html);
        assert_eq!(
            lines,
            vec![
                "Notes",
                "=====",
                "Two things.",
                "- one",
                "- two",
                "More",
                "----"
            ]
        );
    }

    #[test]
    fn a_paragraph_document_becomes_one_line_per_block() {
        let html = "<h1>Report</h1><p>First <b>point</b>.</p><p>Second &amp; last.</p>";
        let lines = lines_from_html(html);
        assert!(lines.contains(&"Report".to_owned()), "{lines:?}");
        assert!(lines.contains(&"First point.".to_owned()), "{lines:?}");
        assert!(lines.contains(&"Second & last.".to_owned()), "{lines:?}");
    }

    #[test]
    fn this_project_s_own_markup_is_read_before_anything_else() {
        let page = page_of(&["│ Resume Session   Ctrl+R │"]);
        let html = render_page(&page, "t");
        let lines = lines_from_html(&html);
        assert!(!lines.is_empty());
        assert!(
            lines.iter().any(|line| line.contains("Resume Session")),
            "{lines:?}"
        );
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
