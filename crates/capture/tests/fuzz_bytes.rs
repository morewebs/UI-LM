//! Any bytes at all: whatever a program writes, the recorder must produce a frame it can store.
//!
//! The emulator is `vt100`, which is not this project's code — but turning its screen into a `Frame`
//! is. A program can write escape sequences, half-finished control strings, invalid UTF-8 and bytes
//! nobody meant to send, and the recorder still has to write a session. This walks a spread of such
//! writes and asks two things: `frame_from_screen` returns, and the frame it makes describes its own
//! screen — every run inside the grid, every row and column within the size it claims.

use ui_lm_capture::screen::frame_from_screen;

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.0 >> 33
    }
}

#[test]
fn no_bytes_make_an_inconsistent_frame() {
    let fragments: [&[u8]; 16] = [
        b"hello",
        b"\x1b[31mred\x1b[0m",
        b"\x1b[2J",
        b"\x1b[H",
        b"\x1b[10;20Hmoved",
        b"\x1b]0;title\x07",
        b"\x1b[?25lhidden",
        b"\x1b(B",
        b"\xff\xfe\xfd",
        b"\xe6\x97\xa5\xe6\x9c\xac",
        b"\t\t tabs",
        b"\r\n\r\n",
        b"\x1b[1;1H\x1b[K",
        b"\x00\x01\x02",
        b"\x1b[999;999Hfar",
        b"\x1b7\x1b8",
    ];
    let mut rng = Rng(0xfeed);
    let mut problems = Vec::new();

    for index in 0..300 {
        let (rows, cols) = (2 + (rng.next() % 22) as u16, 2 + (rng.next() % 78) as u16);
        let mut parser = vt100::Parser::new(rows, cols, 0);
        for _ in 0..(rng.next() % 12) {
            parser.process(fragments[(rng.next() as usize) % fragments.len()]);
        }
        let frame = frame_from_screen(
            parser.screen(),
            index as u32,
            None,
            false,
            None,
            None,
            vec![],
        );

        let (claimed_rows, claimed_cols) = (frame.rows, frame.cols);
        if claimed_rows != rows || claimed_cols != cols {
            problems.push(format!(
                "screen {index}: frame says {claimed_rows}x{claimed_cols}, screen {rows}x{cols}"
            ));
        }
        for run in &frame.runs {
            if usize::from(run.row) >= usize::from(rows)
                || usize::from(run.col) + usize::from(run.len) > usize::from(cols)
            {
                problems.push(format!(
                    "screen {index}: run at row {} col {} len {} leaves a {rows}x{cols} screen",
                    run.row, run.col, run.len
                ));
            }
        }
    }

    assert!(
        problems.is_empty(),
        "{} frame(s) describe their screen badly:\n{}",
        problems.len(),
        problems
            .iter()
            .take(3)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
}
