//! Driving a child program inside a pseudo terminal.
//!
//! The driver owns the whole capture side of a session: it spawns the program on
//! a pty, feeds every byte the program writes through a `vt100` emulator, and
//! turns the emulator's screen into something a frame can be read from. Keystrokes
//! go the other way through [`Driver::write_input`].
//!
//! Reading is the part that cannot be done naively. `portable-pty` offers no
//! portable non-blocking read — ConPTY has none at all — so the pty is read by a
//! background thread that hands chunks over a channel. [`Driver::read_some`] then
//! never waits for output: it reports the bytes that have arrived,
//! [`io::ErrorKind::WouldBlock`] while the session is still live but quiet, and
//! `Ok(0)` only once the stream has genuinely ended. A caller's loop therefore
//! decides for itself when to emit a frame, and a slow child cannot turn into a
//! truncated session.
//!
//! Two behaviours of the stream are the driver's responsibility rather than the
//! caller's, because they are what makes a pty host a host:
//!
//! - **The end of the stream.** A pseudo terminal's read does not report EOF when
//!   the child exits; it reports EOF when the pty itself is closed. So once the
//!   child is known to be gone the driver closes its own handle, the reader
//!   drains what is already buffered and then finishes, and `Ok(0)` follows from
//!   the reader having ended — never from a moment of quiet.
//! - **Answering the child's cursor-position query.** A terminal is expected to
//!   answer `ESC [ 6 n`, and ConPTY's client does not start until it gets an
//!   answer, so a host that never replies captures nothing at all.

use std::collections::VecDeque;
use std::io::{self, Read, Write};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::thread::JoinHandle;

use anyhow::{Context, Result};
use portable_pty::{Child, CommandBuilder, MasterPty, PtySize};

/// The child's terminal environment unless the caller overrides it.
///
/// A capture rig has to look like the terminal an application expects: without a
/// `TERM` that promises colour and a `COLORTERM` that promises 24-bit colour,
/// applications downgrade or refuse to draw, and the recorded frames would not be
/// the frames the application draws for a real user.
const DEFAULT_ENV: [(&str, &str); 2] = [("TERM", "xterm-256color"), ("COLORTERM", "truecolor")];

/// Size of the buffer the reader thread hands over at a time.
const READ_CHUNK: usize = 8 * 1024;

/// What to run, and the screen to emulate it on.
#[derive(Clone, Debug)]
pub struct SessionConfig {
    /// The program to run.
    pub command: String,
    /// Its arguments, not including the program name.
    pub args: Vec<String>,
    /// Emulated screen height, in rows.
    pub rows: u16,
    /// Emulated screen width, in columns.
    pub cols: u16,
    /// Environment overrides for the child, applied on top of the inherited
    /// environment and taking precedence over [`DEFAULT_ENV`].
    pub env: Vec<(String, String)>,
}

/// A child process on a pty, and the emulator its output is fed to.
pub struct Driver {
    /// The pty master: resizing goes through it, and it must outlive the child.
    ///
    /// Held in an `Option` so it can be closed deliberately: on ConPTY, closing
    /// it is what ends the reader's stream once the child has gone.
    master: Option<Box<dyn MasterPty + Send>>,
    /// Writing here reaches the child's stdin. Also optional so that `drop` can
    /// close it — which is the child's EOF — before joining the reader.
    writer: Option<Box<dyn Write + Send>>,
    /// The spawned program, polled for its exit status.
    child: Box<dyn Child + Send + Sync>,
    /// The terminal emulator: every byte read from the pty is processed into it.
    ///
    /// Sized `rows x cols` with no scrollback: the rig records the viewport, and
    /// scrollback is the application's own concern.
    parser: vt100::Parser,
    /// Chunks handed over by the reader thread. The channel disconnecting — every
    /// sender gone, which happens when that thread ends — is how the stream's end
    /// is reported; an empty channel on its own means nothing more than "nothing
    /// has arrived yet".
    chunks: Receiver<Vec<u8>>,
    /// Bytes received from the channel but not yet handed to a caller.
    pending: VecDeque<u8>,
    /// How much of [`CURSOR_QUERY`] has been matched so far, so a query split
    /// across two reads is still recognised.
    query_progress: usize,
    /// The reader thread. Joined in `drop`, so it never outlives the driver.
    reader: Option<JoinHandle<()>>,
    /// Set once the reader thread has ended: from then on an empty `pending` is
    /// EOF rather than "wait a little longer".
    eof: bool,
}

impl Driver {
    /// Spawns `cfg`'s program on a new pty at the configured size and starts
    /// reading its output.
    ///
    /// The child inherits this process's environment, then [`DEFAULT_ENV`] is
    /// applied, then `cfg.env` — so the caller can override the defaults.
    pub fn spawn(cfg: &SessionConfig) -> Result<Self> {
        let pair = portable_pty::native_pty_system()
            .openpty(pty_size(cfg.rows, cfg.cols))
            .context("failed to open a pty")?;

        let mut cmd = CommandBuilder::new(&cfg.command);
        cmd.args(&cfg.args);
        for (key, value) in DEFAULT_ENV {
            cmd.env(key, value);
        }
        for (key, value) in &cfg.env {
            cmd.env(key, value);
        }

        let child = pair
            .slave
            .spawn_command(cmd)
            .with_context(|| format!("failed to spawn {:?} on the pty", cfg.command))?;
        // The slave end exists only in the child from here on. Keeping our copy
        // open would stop the pty ever reaching EOF.
        drop(pair.slave);

        // Taking the writer matters even when no input is ever sent: dropping it
        // sends EOF to the child, and not taking it would leave the child waiting
        // for input that cannot arrive.
        let writer = pair
            .master
            .take_writer()
            .context("failed to take the pty writer")?;
        let mut reader = pair
            .master
            .try_clone_reader()
            .context("failed to clone the pty reader")?;

        let (tx, chunks) = mpsc::channel();
        let thread = std::thread::spawn(move || {
            let mut buf = [0u8; READ_CHUNK];
            loop {
                match reader.read(&mut buf) {
                    // A pty read of zero bytes is the end of the stream.
                    Ok(0) => return,
                    Ok(n) => {
                        // The receiver is gone only once the driver is dropping,
                        // and it joins this thread before dropping it.
                        if tx.send(buf[..n].to_vec()).is_err() {
                            return;
                        }
                    }
                    Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                    // A read error ends the stream just as EOF does: there is
                    // nothing more to record, and a pty whose child has gone is
                    // the common one, not a failure worth reporting.
                    Err(_) => return,
                }
            }
        });

        Ok(Self {
            master: Some(pair.master),
            writer: Some(writer),
            child,
            parser: vt100::Parser::new(cfg.rows, cfg.cols, 0),
            chunks,
            pending: VecDeque::new(),
            query_progress: 0,
            reader: Some(thread),
            eof: false,
        })
    }

    /// Copies the output that has arrived into `buf` and processes it into the
    /// emulator, returning how many bytes were copied.
    ///
    /// - `Ok(n)` with `n > 0`: bytes were available.
    /// - [`io::ErrorKind::WouldBlock`]: the stream is still open but empty right
    ///   now. This is the caller's cue to sleep and try again, or to emit a frame
    ///   for whatever the screen has settled on.
    /// - `Ok(0)`: the stream has ended — the reader thread finished and every
    ///   byte it read has been handed over. Never returned merely because the
    ///   channel is momentarily empty, since that would truncate a slow child.
    ///
    /// `Ok(0)` is never returned merely because `buf` is empty: an empty buffer
    /// with bytes waiting cannot carry them, and reporting progress of zero would
    /// end the caller's loop, so that is `WouldBlock` as well. `Ok(0)` therefore
    /// always means the stream has ended.
    pub fn read_some(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.fill_pending();
        let n = buf.len().min(self.pending.len());
        if n == 0 {
            if self.eof {
                return Ok(0);
            }
            // Nothing buffered. If the child is gone the session is over, and
            // closing the pty is what lets the reader reach the end of the
            // stream; the bytes still in flight are delivered on the next call.
            self.close_pty_if_child_gone();
            return Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                "no output from the pty yet",
            ));
        }
        for (slot, byte) in buf[..n].iter_mut().zip(self.pending.drain(..n)) {
            *slot = byte;
        }
        self.parser.process(&buf[..n]);
        self.answer_cursor_queries(&buf[..n]);
        Ok(n)
    }

    /// The emulator's current screen: what the child's output has drawn so far.
    pub fn screen(&self) -> &vt100::Screen {
        self.parser.screen()
    }

    /// Sends `bytes` to the child's stdin, as though they had been typed.
    pub fn write_input(&mut self, bytes: &[u8]) -> io::Result<()> {
        self.write_raw(bytes)
    }

    /// Resizes both the pty and the emulator: the child learns the new size, and
    /// the screen the frames are read from is resized with it, so the frame and
    /// the program can never disagree about the geometry.
    ///
    /// Resizing a session that has already ended is an error: there is no longer
    /// a program whose screen this could be.
    pub fn resize(&mut self, rows: u16, cols: u16) -> Result<()> {
        let master = self
            .master
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("cannot resize a session that has ended"))?;
        master
            .resize(pty_size(rows, cols))
            .context("failed to resize the pty")?;
        self.parser.set_size(rows, cols);
        Ok(())
    }

    /// The child's exit code, or `None` while it is still running. Never blocks.
    pub fn try_wait(&mut self) -> Result<Option<i32>> {
        let status = self.child.try_wait().context("failed to poll the child")?;
        Ok(status.map(|status| status.exit_code() as i32))
    }

    /// Moves chunks from the reader thread into `pending` until there is
    /// something to hand over or the stream ends.
    fn fill_pending(&mut self) {
        while self.pending.is_empty() {
            match self.chunks.try_recv() {
                Ok(chunk) => self.pending.extend(chunk),
                Err(TryRecvError::Empty) => return,
                Err(TryRecvError::Disconnected) => {
                    self.eof = true;
                    return;
                }
            }
        }
    }

    /// Closes this end of the pty once the child has exited.
    ///
    /// A pty's read reports EOF when the pty is closed, not when the child exits,
    /// and on ConPTY the host process stays alive for as long as the pty does.
    /// Closing our handle is therefore the only way the reader can see the end of
    /// the stream; the reader's own duplicated handle keeps whatever is still
    /// buffered readable until it has been drained, so nothing is lost by closing
    /// first.
    ///
    /// The child is polled here rather than waited for, so this never blocks on a
    /// program that is still running.
    fn close_pty_if_child_gone(&mut self) {
        if self.master.is_none() {
            return;
        }
        if matches!(self.child.try_wait(), Ok(Some(_))) {
            self.master.take();
        }
    }

    /// Answers every cursor-position query in `bytes`, so a program that asks the
    /// terminal where the cursor is gets an answer instead of waiting for one.
    ///
    /// The query is `ESC [ 6 n` (DSR 6). The match is stateful because its bytes
    /// can be split across reads. A failed reply is not reported: it means the
    /// child has gone, and the read side reports the end of the stream on its own.
    ///
    /// `vt100` is a display model with no way to reply, so this cannot be left to
    /// the emulator — and a host that never answers captures nothing at all, since
    /// ConPTY's client does not start until its opening query is answered.
    fn answer_cursor_queries(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            self.query_progress = match (self.query_progress, byte) {
                (_, b'\x1b') => 1,
                (1, b'[') => 2,
                (2, b'6') => 3,
                (3, b'n') => {
                    let (row, col) = self.parser.screen().cursor_position();
                    // DSR replies are one-based.
                    let reply = format!("\x1b[{};{}R", row + 1, col + 1);
                    let _ = self.write_raw(reply.as_bytes());
                    0
                }
                _ => 0,
            };
        }
    }

    /// Writes straight to the pty: the one place bytes enter the child's input.
    fn write_raw(&mut self, bytes: &[u8]) -> io::Result<()> {
        let writer = self
            .writer
            .as_mut()
            .ok_or_else(|| io::Error::new(io::ErrorKind::BrokenPipe, "the pty writer is closed"))?;
        writer.write_all(bytes)
    }
}

impl Drop for Driver {
    /// Ends the session without leaving anything running behind it.
    ///
    /// The write side is closed first, so the child sees EOF and a well-behaved
    /// program exits; a child that ignores it is terminated, because it is holding
    /// the pty open and the reader thread's read would otherwise never return. The
    /// pty is then closed, and the thread joined before the channel's receiver goes
    /// away, so it can never be left appending to a channel whose other end has
    /// been dropped.
    fn drop(&mut self) {
        self.writer.take();
        if matches!(self.child.try_wait(), Ok(None)) {
            let _ = self.child.kill();
        }
        self.master.take();
        if let Some(thread) = self.reader.take() {
            let _ = thread.join();
        }
    }
}

/// A pty of `rows x cols`, with no pixel geometry: nothing here knows the size of
/// a cell, and the rig records character geometry only.
fn pty_size(rows: u16, cols: u16) -> PtySize {
    PtySize {
        rows,
        cols,
        pixel_width: 0,
        pixel_height: 0,
    }
}
