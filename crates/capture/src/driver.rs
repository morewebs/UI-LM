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
//! Nothing on the read path writes to the child. `portable-pty` offers no
//! non-blocking write either, and a write to a child that has stopped reading its
//! input blocks until the child reads — so answering a cursor query inline would
//! make the read path wait on the child's stdin, a hang inside the one function a
//! recorder relies on never to hang. Every write is queued instead, and one writer
//! thread drains the queue: it is the only thing that touches the pty's write side.
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
//!   answer, so a host that never replies captures nothing at all. Each reply
//!   carries the cursor position as of the point the query appears in the stream
//!   — the position the child asked about — and is queued, so that a child which
//!   floods queries without reading the answers cannot stall the read path; the
//!   queue is bounded, and replies that do not fit are dropped and counted.
//!
//! The child's working directory is the third thing a host decides rather than
//! inherits. `portable-pty` gives a command that names no directory the user's home
//! directory, and a capture must never run a program there: a session started inside
//! a scratch project recorded the home directory's tool screens and could create a
//! file in it. So [`SessionConfig::working_directory`] — an explicit directory if the
//! caller named one, the recorder's own current directory otherwise — is always what
//! the command builder is given.

use std::collections::VecDeque;
use std::io::{self, Read, Write};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, SyncSender, TryRecvError, TrySendError};
use std::thread::JoinHandle;

use anyhow::{Context, Result};
use portable_pty::{Child, CommandBuilder, MasterPty, PtySize};

/// The child's terminal environment unless the caller overrides it.
///
/// A capture rig has to look like the terminal an application expects: without a
/// `TERM` that promises colour and a `COLORTERM` that promises 24-bit colour,
/// applications downgrade or refuse to draw, and the recorded frames would not be
/// the frames the application draws for a real user.
///
/// Public because it is also part of what a session records: the environment a
/// recorded screen was drawn in is not reproducible from the caller's overrides
/// alone, and the recorder reads these from here rather than keeping a second copy
/// of the defaults that could drift from the ones the child actually got.
pub const DEFAULT_ENV: [(&str, &str); 2] = [("TERM", "xterm-256color"), ("COLORTERM", "truecolor")];

/// Size of the buffer the reader thread hands over at a time.
const READ_CHUNK: usize = 8 * 1024;

/// How many writes may be waiting for the writer thread at once.
///
/// The ceiling exists for the one case the read path cannot control: a child that
/// stops reading its input. A cursor-query reply is a few bytes and a flood of
/// queries produces one reply per four bytes of output, so an unbounded queue
/// would let such a child grow this process's memory without limit. This is also
/// the cap on pending replies, since a reply is one of these writes: at most 64
/// can ever be waiting, and one that does not fit is dropped and counted. Caller
/// input shares the same ceiling rather than having one of its own. 64 pending
/// writes is far more than a live session needs and is a fixed, small cost.
const WRITE_QUEUE_CAP: usize = 64;

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
    /// The directory the child runs in.
    ///
    /// `None` means the recorder's own current directory, which is what a capture
    /// started inside a scratch project means by "here". Portable-pty's own default
    /// for a command that names no directory is the user's home directory, and
    /// that is never the right one: a session started in a scratch project recorded
    /// the home directory's tool screens and could create a file there.
    pub cwd: Option<PathBuf>,
}

impl SessionConfig {
    /// The directory the child runs in: [`SessionConfig::cwd`] if the caller named
    /// one, and the recorder's own current directory otherwise.
    ///
    /// This is deliberately not `cwd.or(home)`. `portable-pty`'s own resolution is
    /// exactly that — `CommandBuilder::current_directory` is
    /// `self.cwd.or(USERPROFILE)` — and it is the reason a W1 session started in a
    /// scratch project still recorded the home directory: the tool searches found
    /// nothing, the screens were the home directory's, and one scenario created a
    /// file there. A capture tool that spawns an application in the user's home is
    /// a defect, not a documented behaviour.
    ///
    /// An explicit directory that is not one is an error rather than a fallback:
    /// `portable-pty` silently ignores a path that is not a directory and drops to
    /// the home directory, so a typo in the flag would land exactly where this
    /// method exists to prevent, and would do it quietly.
    pub fn working_directory(&self) -> Result<PathBuf> {
        let directory = match &self.cwd {
            Some(directory) => directory.clone(),
            None => std::env::current_dir().context(
                "the recorder's own working directory could not be read, so the child has no \
                 directory to default to",
            )?,
        };
        if !directory.is_dir() {
            anyhow::bail!(
                "the child's working directory {} is not a directory",
                directory.display()
            );
        }
        Ok(directory)
    }
}

/// A child process on a pty, and the emulator its output is fed to.
pub struct Driver {
    /// The pty master: resizing goes through it, and it must outlive the child.
    ///
    /// Held in an `Option` so it can be closed deliberately: on ConPTY, closing
    /// it is what ends the reader's stream once the child has gone.
    master: Option<Box<dyn MasterPty + Send>>,
    /// The child's input: every write this driver makes goes through here, and no
    /// other part of the driver can reach the pty's write side.
    writes: WriteQueue,
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
    /// How much of the cursor query has been matched so far, so a query split
    /// across two reads is still recognised.
    query_progress: usize,
    /// The thread that performs every write to the child, so that producing a
    /// write never waits for the child to read.
    writer: Option<JoinHandle<()>>,
    /// The reader thread. Joined in `drop`, so it never outlives the driver.
    reader: Option<JoinHandle<()>>,
    /// Set once the reader thread has ended: from then on an empty `pending` is
    /// EOF rather than "wait a little longer".
    eof: bool,
}

/// One write the writer thread owes the child.
enum WriteOp {
    /// An answer to a cursor-position query.
    Reply(Vec<u8>),
    /// Bytes a caller handed to [`Driver::write_input`].
    Input(Vec<u8>),
}

/// The hand-off between everything that produces writes and the one thread that
/// performs them.
///
/// Nothing here blocks, and nothing here grows without limit: a write that does
/// not fit is refused. For a cursor-query reply that refusal is final and counted
/// — the child is not reading, and there is nothing useful to do about it — while
/// for caller input it is reported, so the caller decides whether to retry.
struct WriteQueue {
    /// The channel to the writer thread; `None` once the session is over.
    sender: Option<SyncSender<WriteOp>>,
    /// Cursor-query replies refused because the queue was full.
    dropped_replies: usize,
}

impl WriteQueue {
    fn new(sender: SyncSender<WriteOp>) -> Self {
        Self {
            sender: Some(sender),
            dropped_replies: 0,
        }
    }

    /// Queues `reply` for the child, or drops it if it cannot be queued.
    ///
    /// Returns whether it was queued. Never waits, so the read path cannot be
    /// stalled by a child that does not read its input; a reply the writer cannot
    /// take — the queue is full, or the write side has already gone — is lost and
    /// counts towards [`WriteQueue::dropped`]. Nothing here reports a failure: an
    /// answer that cannot be delivered is not an error the read path could act on.
    fn enqueue_reply(&mut self, reply: &str) -> bool {
        let Some(sender) = self.sender.as_ref() else {
            return false;
        };
        match sender.try_send(WriteOp::Reply(reply.as_bytes().to_vec())) {
            Ok(()) => true,
            Err(_) => {
                self.dropped_replies += 1;
                false
            }
        }
    }

    /// Queues `bytes` for the child's stdin, reporting rather than waiting when
    /// the writer is behind.
    ///
    /// [`io::ErrorKind::WouldBlock`] means the queue is full and the caller may
    /// retry; [`io::ErrorKind::BrokenPipe`] means the session's write side has
    /// closed.
    fn enqueue_input(&mut self, bytes: &[u8]) -> io::Result<()> {
        let Some(sender) = self.sender.as_ref() else {
            return Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "the pty write side is closed",
            ));
        };
        match sender.try_send(WriteOp::Input(bytes.to_vec())) {
            Ok(()) => Ok(()),
            Err(TrySendError::Full(_)) => Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                "the pty write queue is full",
            )),
            Err(TrySendError::Disconnected(_)) => Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "the pty write side is closed",
            )),
        }
    }

    /// How many cursor-query replies have been dropped because the queue was full.
    fn dropped(&self) -> usize {
        self.dropped_replies
    }

    /// Ends the queue: once the sender is gone the writer thread's `recv` fails,
    /// and the thread drops the pty writer, which is the child's EOF.
    fn close(&mut self) {
        self.sender = None;
    }
}

impl Driver {
    /// Spawns `cfg`'s program on a new pty at the configured size and starts
    /// reading its output.
    ///
    /// The child inherits this process's environment, then [`DEFAULT_ENV`] is
    /// applied, then `cfg.env` — so the caller can override the defaults. It runs in
    /// [`SessionConfig::working_directory`], which is the caller's directory or the
    /// recorder's own, never `portable-pty`'s home-directory default.
    pub fn spawn(cfg: &SessionConfig) -> Result<Self> {
        let pair = portable_pty::native_pty_system()
            .openpty(pty_size(cfg.rows, cfg.cols))
            .context("failed to open a pty")?;

        let mut cmd = CommandBuilder::new(&cfg.command);
        cmd.args(&cfg.args);
        // Set explicitly, always: leaving it unset is what makes the child run in
        // the user's home directory.
        cmd.cwd(cfg.working_directory()?);
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
        // for input that cannot arrive. It goes straight to the writer thread,
        // which is the only thing that ever touches it.
        let writer = pair
            .master
            .take_writer()
            .context("failed to take the pty writer")?;
        let mut reader = pair
            .master
            .try_clone_reader()
            .context("failed to clone the pty reader")?;

        let (write_ops, ops) = mpsc::sync_channel(WRITE_QUEUE_CAP);
        let writer_thread = std::thread::Builder::new()
            .name("pty-writer".into())
            .spawn(move || {
                let mut writer = writer;
                while let Ok(op) = ops.recv() {
                    let bytes = match op {
                        WriteOp::Reply(bytes) | WriteOp::Input(bytes) => bytes,
                    };
                    // A failed write means the child has gone and nothing on this
                    // side can be delivered to it; the read side is what reports
                    // the end of the stream, so stop rather than spin.
                    if writer.write_all(&bytes).is_err() {
                        break;
                    }
                }
                // Dropping the writer closes the child's stdin: its EOF.
            })
            .context("failed to spawn the pty writer thread")?;

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
            writes: WriteQueue::new(write_ops),
            child,
            parser: vt100::Parser::new(cfg.rows, cfg.cols, 0),
            chunks,
            pending: VecDeque::new(),
            query_progress: 0,
            writer: Some(writer_thread),
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
    ///
    /// Nothing here waits on the child, in any direction: no output is waited for
    /// (that is `WouldBlock`), the child's exit is polled rather than waited for,
    /// and the answers it is owed are queued for the writer thread instead of
    /// being written here. A child that stops reading its input therefore cannot
    /// stall a caller's record loop. The one thing beyond moving bytes is closing
    /// this end of the pty once the child has exited — bounded, platform-side work
    /// that is the only way the reader reaches the end of the stream.
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
        let Self {
            parser,
            query_progress,
            writes,
            ..
        } = self;
        process_output(parser, query_progress, &buf[..n], |reply| {
            writes.enqueue_reply(reply);
        });
        Ok(n)
    }

    /// The emulator's current screen: what the child's output has drawn so far.
    pub fn screen(&self) -> &vt100::Screen {
        self.parser.screen()
    }

    /// Queues `bytes` to be sent to the child's stdin, as though they had been
    /// typed.
    ///
    /// The write itself happens on the writer thread, so this never waits for a
    /// child that is not reading its input: [`io::ErrorKind::WouldBlock`] means
    /// the queue is full and the bytes were not queued — the caller may retry —
    /// and [`io::ErrorKind::BrokenPipe`] means the session's write side has
    /// closed. A write that fails at the pty itself is not reported here either;
    /// the read side is what reports the end of the stream. Input and
    /// cursor-query replies share one bounded queue, so they reach the child in
    /// the order the session produced them.
    pub fn write_input(&mut self, bytes: &[u8]) -> io::Result<()> {
        self.writes.enqueue_input(bytes)
    }

    /// How many cursor-position replies were dropped because they could not be
    /// queued — the write queue was full, or the session's write side had already
    /// closed.
    ///
    /// Zero for any child that reads what it is sent. A non-zero count means a
    /// child asked about the cursor and did not collect the answers; the replies
    /// are dropped rather than queued without limit, so the session continues.
    pub fn dropped_replies(&self) -> usize {
        self.writes.dropped()
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
}

/// Feeds `bytes` through `parser`, reporting the reply owed for each
/// cursor-position query at the point the query appears in the stream.
///
/// The query is `ESC [ 6 n` (DSR 6). The match is stateful — `progress` carries
/// how much of the sequence has been matched — because its bytes can be split
/// across reads, and the emulator is fed up to the query's last byte before the
/// answer is taken, so a query followed by cursor movement in the same chunk gets
/// the position the child asked about rather than the one the movement left
/// behind. Every byte is processed exactly once.
///
/// `vt100` is a display model with no way to reply, so this cannot be left to the
/// emulator — and a host that never answers captures nothing at all, since
/// ConPTY's client does not start until its opening query is answered.
fn process_output(
    parser: &mut vt100::Parser,
    progress: &mut usize,
    bytes: &[u8],
    mut on_query: impl FnMut(&str),
) {
    let mut fed = 0;
    for (index, &byte) in bytes.iter().enumerate() {
        *progress = match (*progress, byte) {
            (_, b'\x1b') => 1,
            (1, b'[') => 2,
            (2, b'6') => 3,
            (3, b'n') => {
                parser.process(&bytes[fed..=index]);
                fed = index + 1;
                let (row, col) = parser.screen().cursor_position();
                // DSR replies are one-based.
                on_query(&format!("\x1b[{};{}R", row + 1, col + 1));
                0
            }
            _ => 0,
        };
    }
    if fed < bytes.len() {
        parser.process(&bytes[fed..]);
    }
}

impl Drop for Driver {
    /// Ends the session without leaving anything running behind it.
    ///
    /// The write queue is closed first, so the writer thread drops the pty's write
    /// side — the child's EOF — and the reader's stream can end; a child that
    /// ignores it is terminated, because it is holding the pty open and the reader
    /// thread's read would otherwise never return. The pty is then closed, which
    /// also releases a writer blocked on a child that stopped reading, and both
    /// threads are joined before the channels' receivers go away, so neither can
    /// be left sending into a channel whose other end has been dropped.
    fn drop(&mut self) {
        self.writes.close();
        if matches!(self.child.try_wait(), Ok(None)) {
            let _ = self.child.kill();
        }
        self.master.take();
        if let Some(thread) = self.writer.take() {
            let _ = thread.join();
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    /// A config that is never spawned: only the fields the working directory
    /// depends on matter here.
    fn config_with_cwd(cwd: Option<PathBuf>) -> SessionConfig {
        SessionConfig {
            command: "true".to_owned(),
            args: Vec::new(),
            rows: 5,
            cols: 20,
            env: Vec::new(),
            cwd,
        }
    }

    #[test]
    fn a_configured_directory_is_used_and_one_that_is_not_a_directory_is_refused() {
        // `portable-pty` filters the command's directory with `is_dir` and falls
        // back to the home directory when that filter rejects it, so a helper that
        // quietly passed a bad path through would put the child in the user's home.
        // A path that is not a directory has to be this rig's error instead.
        let scratch = tempfile::tempdir().expect("a temporary directory");
        let named = config_with_cwd(Some(scratch.path().to_path_buf()));
        assert_eq!(
            named.working_directory().unwrap(),
            scratch.path(),
            "the caller's own directory must be what the child gets"
        );

        let missing = scratch.path().join("not-a-directory");
        let broken = config_with_cwd(Some(missing.clone()));
        let error = broken
            .working_directory()
            .expect_err("a directory that does not exist is not a directory to run a child in");
        let message = format!("{error:#}");
        assert!(
            message.contains(&missing.display().to_string()),
            "the error must name the directory that was asked for: {message:?}"
        );
    }

    /// The replies `read_some` would enqueue for `bytes`, in stream order.
    fn replies_for(parser: &mut vt100::Parser, progress: &mut usize, bytes: &[u8]) -> Vec<String> {
        let mut replies = Vec::new();
        process_output(parser, progress, bytes, |reply| {
            replies.push(reply.to_owned());
        });
        replies
    }

    #[test]
    fn a_cursor_query_is_answered_where_the_query_appears_in_the_stream() {
        // "A" leaves the cursor on row 1, column 2 (one-based), so that is the
        // position the query asks about. The escape after it moves the cursor to
        // (5, 7); answering from the end of the chunk would claim 5;7, a cursor
        // the child never had, and a program that saves that position to restore
        // it later would restore the wrong one.
        let mut parser = vt100::Parser::new(5, 20, 0);
        let mut progress = 0;
        let replies = replies_for(&mut parser, &mut progress, b"A\x1b[6n\x1b[5;7H");
        assert_eq!(replies, ["\x1b[1;2R"]);
        assert_eq!(parser.screen().cursor_position(), (4, 6));
    }

    #[test]
    fn a_cursor_query_split_across_two_reads_is_answered_once() {
        // The sequence can arrive cut in half; the stateful match must still
        // recognise it, answer it once, and use the position from before the
        // movement that follows it in the second read.
        let mut parser = vt100::Parser::new(5, 20, 0);
        let mut progress = 0;
        assert!(replies_for(&mut parser, &mut progress, b"A\x1b[6").is_empty());
        let replies = replies_for(&mut parser, &mut progress, b"n\x1b[5;7H");
        assert_eq!(replies, ["\x1b[1;2R"]);
        assert_eq!(parser.screen().cursor_position(), (4, 6));
    }

    #[test]
    fn the_reply_queue_stops_at_its_cap_and_counts_what_it_drops() {
        // No writer thread here: the queue is exactly as full as the pushes make
        // it, which is the situation the read path has to survive — a child that
        // has stopped reading the answers it asked for. The test would hang if
        // enqueueing waited for room, which is the point of it.
        assert_eq!(WRITE_QUEUE_CAP, 64);
        let (sender, held_open) = mpsc::sync_channel(WRITE_QUEUE_CAP);
        let mut queue = WriteQueue::new(sender);
        for queued in 0..WRITE_QUEUE_CAP {
            assert!(
                queue.enqueue_reply("\x1b[1;2R"),
                "reply {queued} should still fit below the cap"
            );
        }
        assert_eq!(queue.dropped(), 0);
        assert!(
            !queue.enqueue_reply("\x1b[1;2R"),
            "a reply past the cap must be refused, not queued"
        );
        assert!(!queue.enqueue_reply("\x1b[1;2R"));
        assert_eq!(queue.dropped(), 2);
        drop(held_open);
    }

    #[test]
    fn a_closed_queue_refuses_writes_instead_of_panicking() {
        let (sender, held_open) = mpsc::sync_channel(WRITE_QUEUE_CAP);
        let mut queue = WriteQueue::new(sender);
        queue.close();
        assert!(!queue.enqueue_reply("\x1b[1;2R"));
        let error = queue
            .enqueue_input(b"abc")
            .expect_err("input after the queue closed should be an error");
        assert_eq!(error.kind(), io::ErrorKind::BrokenPipe);
        drop(held_open);
    }
}
