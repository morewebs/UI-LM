//! What a set of recorded sessions actually contains.
//!
//! A capture produces a directory per session, and the questions asked of a corpus
//! afterwards are always the same: how many screens, how many of them are the same
//! screen seen twice, how much of the change was masked as noise, and which events
//! the application labelled. Reading that off `frames.jsonl` by hand is slow and
//! easy to get wrong, so it is computed once here and printed.

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};

use ui_lm_frames::{decode_grid, read_frames, read_orphaned_frames, Frame};

/// The counts a corpus is argued about with.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Summary {
    /// Directories read, whether they held a finished session or a killed one.
    pub sessions: usize,
    /// Frames across every session.
    pub frames: usize,
    /// Frames that describe a screen no other frame in the corpus describes.
    pub distinct_screens: usize,
    /// Cells marked as noise by the volatile mask.
    pub masked_cells: usize,
    /// Frames that could name the stretch of the event stream behind them.
    pub frames_with_range: usize,
    /// Frames that carried the application's own labels.
    pub frames_with_kinds: usize,
    /// Earliest and latest frame offset seen, in milliseconds.
    pub first_at_ms: Option<u64>,
    pub last_at_ms: Option<u64>,
    /// How often the application labelled an event with each kind.
    pub kinds: BTreeMap<String, usize>,
}

impl Summary {
    /// Adds one session's frames to the totals.
    ///
    /// Kept separate from reading them so counting is testable without a corpus on
    /// disk, and so a caller holding frames for another reason can reuse it.
    pub fn add(&mut self, frames: &[Frame]) {
        self.frames += frames.len();
        let mut seen: HashSet<String> = HashSet::new();
        for frame in frames {
            // A screen is its cells: two frames with the same size and the same runs
            // are the same screen, whatever their sequence numbers say.
            seen.insert(format!("{}x{}|{:?}", frame.rows, frame.cols, frame.runs));
            self.masked_cells += frame
                .volatile
                .iter()
                .map(|span| usize::from(span.len))
                .sum::<usize>();
            if frame.acp_event_range.is_some() {
                self.frames_with_range += 1;
            }
            if !frame.event_kinds.is_empty() {
                self.frames_with_kinds += 1;
                for kind in &frame.event_kinds {
                    *self.kinds.entry(kind.clone()).or_default() += 1;
                }
            }
            if let Some(at) = frame.at_ms {
                self.first_at_ms = Some(self.first_at_ms.map_or(at, |seen| seen.min(at)));
                self.last_at_ms = Some(self.last_at_ms.map_or(at, |seen| seen.max(at)));
            }
        }
        self.distinct_screens += seen.len();
    }

    /// The summary as the lines a person reads.
    pub fn render(&self) -> String {
        let span = match (self.first_at_ms, self.last_at_ms) {
            (Some(first), Some(last)) => {
                format!(
                    "{:.1} s of recorded wall clock",
                    (last - first) as f64 / 1000.0
                )
            }
            _ => "no timings recorded".to_owned(),
        };
        let mut out = format!(
            "{} session(s), {} frame(s), {} distinct screen(s)\n\
             {span}\n\
             masked cells: {}; frames naming an event range: {}; frames carrying kinds: {}\n",
            self.sessions,
            self.frames,
            self.distinct_screens,
            self.masked_cells,
            self.frames_with_range,
            self.frames_with_kinds
        );
        if self.kinds.is_empty() {
            out.push_str("events labelled: none\n");
        } else {
            out.push_str("events labelled by the application:\n");
            for (kind, count) in &self.kinds {
                out.push_str(&format!("  {kind}: {count}\n"));
            }
        }
        out
    }
}

/// Summarises each directory under `roots`, finishing sessions and killed ones alike.
///
/// A killed capture is included because the state is honest: its frames are real
/// screens, and dropping them would understate what the corpus holds.
pub fn summarize(roots: &[PathBuf]) -> std::io::Result<Summary> {
    let mut summary = Summary::default();
    for root in roots {
        for dir in session_dirs(root)? {
            let frames = match read_frames(&dir) {
                Ok((_, frames)) => frames,
                Err(_) => read_orphaned_frames(&dir)?,
            };
            summary.sessions += 1;
            summary.add(&frames);
        }
    }
    Ok(summary)
}

/// `root` itself if it looks like a session, else the session directories under it.
fn session_dirs(root: &Path) -> std::io::Result<Vec<PathBuf>> {
    if root.join("frames.jsonl").is_file() {
        return Ok(vec![root.to_path_buf()]);
    }
    let mut found: Vec<PathBuf> = std::fs::read_dir(root)?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.join("frames.jsonl").is_file())
        .collect();
    found.sort();
    Ok(found)
}

/// One way a frame disagrees with itself.
#[derive(Debug, PartialEq, Eq)]
pub struct Violation {
    /// Which session directory the frame was read from.
    pub session: String,
    /// The frame's sequence number within that session.
    pub seq: u32,
    /// What is wrong with it.
    pub problem: String,
}

/// The pages one session directory describes, in frame order.
///
/// A killed capture is included here for the same reason it is summarised: its frames
/// are real screens, and refusing to score them would quietly narrow the corpus.
pub fn session_pages(dir: &Path) -> std::io::Result<Vec<ui_lm_pages::Envelope>> {
    let frames = match read_frames(dir) {
        Ok((_, frames)) => frames,
        Err(_) => read_orphaned_frames(dir)?,
    };
    let surface = dir
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "session".to_owned());
    Ok(ui_lm_pages::pages(&frames, &surface))
}
/// Checks the pages a corpus describes against the promises the page model makes.
///
/// The mask is audited from the frames; this audits what the frames are turned into.
/// A page that names a component the catalogue does not define, loses a child, repeats
/// an id, or puts a span past the end of its line is describable content that no
/// renderer could draw, and finding one here is cheaper than finding it in a renderer.
pub fn audit_pages(roots: &[PathBuf]) -> std::io::Result<Vec<Violation>> {
    let mut violations = Vec::new();
    for root in roots {
        for dir in session_dirs(root)? {
            let frames = match read_frames(&dir) {
                Ok((_, frames)) => frames,
                Err(_) => read_orphaned_frames(&dir)?,
            };
            let surface = dir
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| "session".to_owned());
            for (index, page) in ui_lm_pages::pages(&frames, &surface).iter().enumerate() {
                for problem in ui_lm_pages::audit(page) {
                    violations.push(Violation {
                        session: dir.display().to_string(),
                        seq: u32::try_from(index).unwrap_or(u32::MAX),
                        problem,
                    });
                }
                // The lossless property: a page that cannot be turned back into the
                // screen it describes has dropped something, and every later stage
                // would inherit the loss.
                if let Some(frame) = frames.get(index) {
                    let expected = screen_text(frame);
                    for (row, rendered) in ui_lm_pages::render_text(page).iter().enumerate() {
                        let Some(source) = expected.get(row) else {
                            violations.push(Violation {
                                session: dir.display().to_string(),
                                seq: u32::try_from(index).unwrap_or(u32::MAX),
                                problem: format!(
                                    "the page names row {row}, which the screen does not have"
                                ),
                            });
                            continue;
                        };
                        if rendered != source {
                            violations.push(Violation {
                                session: dir.display().to_string(),
                                seq: u32::try_from(index).unwrap_or(u32::MAX),
                                problem: format!(
                                    "row {row} reads {rendered:?} in the page and {source:?} on \
                                     the screen"
                                ),
                            });
                        }
                    }
                }
            }
        }
    }
    Ok(violations)
}

/// Checks a corpus against the properties the mask promises, and returns what fails.
///
/// The mask's whole claim is that it marks change *nothing* accounts for. That claim
/// is checkable from the frames alone, and until this existed it had only ever been
/// argued:
///
/// - a span must lie inside the frame's own grid, or it describes cells that do not
///   exist;
/// - a span must sit on a row the frame reported as changed, or the mask is pointing
///   at content that did not move;
/// - a frame carrying spans must not have an event range that *grew*, because that is
///   a change the application reported, and masking it would be the failure the mask
///   exists to prevent.
pub fn audit(roots: &[PathBuf]) -> std::io::Result<Vec<Violation>> {
    let mut violations = Vec::new();
    for root in roots {
        for dir in session_dirs(root)? {
            let frames = match read_frames(&dir) {
                Ok((_, frames)) => frames,
                Err(_) => read_orphaned_frames(&dir)?,
            };
            for frame in &frames {
                for problem in frame_problems(frame) {
                    violations.push(Violation {
                        session: dir.display().to_string(),
                        seq: frame.seq,
                        problem,
                    });
                }
            }
        }
    }
    Ok(violations)
}

/// What is wrong with one frame, if anything.
fn frame_problems(frame: &Frame) -> Vec<String> {
    let mut problems = Vec::new();
    let grew = matches!(frame.acp_event_range, Some((from, to)) if to > from);
    if !frame.volatile.is_empty() && grew {
        problems.push(format!(
            "{} masked span(s) while the event stream grew: {:?} reported something \
             behind this change",
            frame.volatile.len(),
            frame.acp_event_range
        ));
    }
    for span in &frame.volatile {
        let ends = u32::from(span.col) + u32::from(span.len);
        if span.row >= frame.rows || ends > u32::from(frame.cols) || span.len == 0 {
            problems.push(format!(
                "span at row {} col {} len {} does not fit a {}x{} screen",
                span.row, span.col, span.len, frame.rows, frame.cols
            ));
        }
        if !frame.changed_rows.contains(&span.row) {
            problems.push(format!(
                "span at row {} is not on a row this frame reported as changed",
                span.row
            ));
        }
    }
    problems
}

/// The text a frame shows, one entry per row, right-trimmed.
///
/// Right-trimmed for the same reason a block's lines are: trailing blanks are the
/// absence of content, not content, and a comparison that counted them would fail on
/// every screen.
/// The text of a screen, one entry per row.
pub fn frame_text(frame: &Frame) -> String {
    screen_text(frame).join("\n")
}

fn screen_text(frame: &Frame) -> Vec<String> {
    decode_grid(&frame.runs, frame.rows, frame.cols)
        .iter()
        .map(|row| {
            row.iter()
                .map(|cell| cell.ch)
                .collect::<String>()
                .trim_end()
                .to_owned()
        })
        .collect()
}

/// One training pair: what a screen said, and the page that describes it.
///
/// This is the shape the project exists to learn. The input is what any program could read
/// from a terminal — the text of the screen, row by row. The target is the page, which is
/// what a runtime draws. Nothing else is offered: no keystrokes, no events, no hidden
/// state, because a model that relied on them would need the recorder to run alongside it,
/// and the point is to convert a screen.
///
/// Frames that describe an empty screen are left out. A blank screen has a page — a page
/// with nothing in it — and teaching a model to produce one from nothing is teaching it
/// the least interesting half of the problem.
pub fn training_pairs(dir: &Path) -> std::io::Result<Vec<(String, String)>> {
    let frames = match read_frames(dir) {
        Ok((_, frames)) => frames,
        Err(_) => read_orphaned_frames(dir)?,
    };
    let surface = dir
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "session".to_owned());
    let pages = ui_lm_pages::pages(&frames, &surface);

    Ok(frames
        .iter()
        .zip(&pages)
        .filter_map(|(frame, page)| {
            let text = screen_text(frame).join("\n");
            if text.trim().is_empty() {
                return None;
            }
            let target = serde_json::to_string(page).ok()?;
            Some((text, target))
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ui_lm_frames::{CellRun, MaskReason, Span, Style};

    /// A frame of one row whose cells read `text`, one cell per character.
    fn frame(seq: u32, text: &str) -> Frame {
        let cols = u16::try_from(text.len()).unwrap();
        let mut frame = Frame::empty(1, cols);
        frame.seq = seq;
        frame.runs = text
            .chars()
            .enumerate()
            .map(|(col, ch)| CellRun {
                row: 0,
                col: u16::try_from(col).unwrap(),
                len: 1,
                ch,
                style: Style::default(),
                wide_tail: false,
            })
            .collect();
        frame
    }

    #[test]
    fn counting_separates_screens_from_frames() {
        let mut summary = Summary::default();
        summary.add(&[frame(0, "ab"), frame(1, "ab"), frame(2, "cd")]);
        assert_eq!(summary.frames, 3);
        assert_eq!(
            summary.distinct_screens, 2,
            "the repeated screen is one screen seen twice"
        );
    }

    #[test]
    fn the_mask_and_the_labels_are_counted_apart() {
        let mut quiet = frame(0, "ab");
        quiet.acp_event_range = Some((0, 10));
        let mut ticking = frame(1, "ab");
        ticking.acp_event_range = Some((10, 10));
        ticking.volatile = vec![Span {
            row: 0,
            col: 0,
            len: 2,
            reason: MaskReason::NoSemanticChange,
        }];
        ticking.event_kinds = vec!["agent_message".to_owned()];
        ticking.at_ms = Some(1_500);

        let mut summary = Summary::default();
        summary.add(&[quiet, ticking]);
        assert_eq!(summary.masked_cells, 2);
        assert_eq!(summary.frames_with_range, 2);
        assert_eq!(summary.frames_with_kinds, 1);
        assert_eq!(summary.kinds.get("agent_message"), Some(&1));
        assert_eq!(summary.first_at_ms, Some(1_500));
    }

    #[test]
    fn the_rendering_names_what_was_counted() {
        let mut summary = Summary::default();
        summary.add(&[frame(0, "a")]);
        let text = summary.render();
        assert!(text.contains("1 frame(s)"), "{text}");
        assert!(text.contains("events labelled: none"), "{text}");
    }

    #[test]
    fn a_masked_change_with_an_event_behind_it_is_a_violation() {
        // The failure the mask exists to prevent, stated as an assertion: a span
        // says "nothing explains this", so a range that grew contradicts it.
        let mut frame = frame(0, "ab");
        frame.acp_event_range = Some((0, 50));
        frame.changed_rows = vec![0];
        frame.volatile = vec![Span {
            row: 0,
            col: 0,
            len: 2,
            reason: MaskReason::NoSemanticChange,
        }];
        let problems = frame_problems(&frame);
        assert_eq!(problems.len(), 1, "{problems:?}");
        assert!(problems[0].contains("event stream grew"), "{problems:?}");
    }

    #[test]
    fn a_span_off_the_changed_rows_is_a_violation() {
        let mut frame = frame(0, "ab");
        frame.acp_event_range = Some((10, 10));
        frame.changed_rows = vec![];
        frame.volatile = vec![Span {
            row: 0,
            col: 0,
            len: 2,
            reason: MaskReason::NoSemanticChange,
        }];
        let problems = frame_problems(&frame);
        assert_eq!(problems.len(), 1, "{problems:?}");
        assert!(problems[0].contains("not on a row"), "{problems:?}");
    }

    #[test]
    fn a_span_that_does_not_fit_the_screen_is_a_violation() {
        let mut frame = frame(0, "ab");
        frame.acp_event_range = Some((10, 10));
        frame.changed_rows = vec![0];
        frame.volatile = vec![Span {
            row: 0,
            col: 1,
            len: 5,
            reason: MaskReason::NoSemanticChange,
        }];
        let problems = frame_problems(&frame);
        assert_eq!(problems.len(), 1, "{problems:?}");
        assert!(problems[0].contains("does not fit"), "{problems:?}");
    }

    #[test]
    fn a_frame_that_agrees_with_itself_has_no_violations() {
        let mut frame = frame(0, "ab");
        frame.acp_event_range = Some((10, 10));
        frame.changed_rows = vec![0];
        frame.volatile = vec![Span {
            row: 0,
            col: 0,
            len: 2,
            reason: MaskReason::NoSemanticChange,
        }];
        assert!(frame_problems(&frame).is_empty());
    }
}
