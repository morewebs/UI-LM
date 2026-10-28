//! A live view of a recording, with typing that reaches the program.
//!
//! The two halves of this project meet here. The recorder observes a terminal program and
//! writes screens to disk; the renderer draws a page. This serves those pages to a browser
//! as they arrive, and sends whatever is typed in the browser back to the program through
//! the recorder's control file. Nothing is emulated: the page you look at is built from the
//! frames the program actually drew, and the keystrokes you type are the ones it receives.
//!
//! # Reach
//!
//! It listens on `127.0.0.1` only, on the port asked for. It is a local development view,
//! not a service: no authentication, no TLS, no bind to another interface. Anyone who can
//! make a request to localhost can type into the recorded program, which is what a person
//! sitting at the machine already can do.
//!
//! # What it is not
//!
//! It does not re-create the terminal's geometry — the page is drawn as HTML by
//! [`ui_lm_render`], so a 160-column screen reflows with the window. It does not stream
//! partial screens either: a page appears when a frame is written, which is when the
//! program settled. A spinner mid-animation is not shown, because it was never a frame.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{Ipv4Addr, SocketAddrV4, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

mod keys;
use std::time::Duration;

use anyhow::{Context, Result};
use ui_lm_pages::Envelope;

/// The command line: `ui-lm-serve --session DIR --keys FILE`.
#[derive(clap::Parser)]
#[command(
    name = "ui-lm-serve",
    version,
    about = "Serves a live view of a recording and types back into the program"
)]
struct Cli {
    /// The recording to watch. Point it at the directory `record --out` wrote.
    #[arg(long, value_name = "DIR")]
    session: PathBuf,

    /// The file the recorder is reading keystrokes from.
    ///
    /// This must be the same path the recorder was given with `--control`; the viewer
    /// appends to it and never touches the pty itself.
    #[arg(long, value_name = "FILE")]
    keys: PathBuf,

    /// The port to listen on, on this machine only.
    #[arg(long, default_value_t = 8787)]
    port: u16,

    /// Describe screens with a trained model instead of the rule-based reader.
    #[arg(long, value_name = "FILE")]
    model: Option<PathBuf>,
}

fn main() {
    let cli = <Cli as clap::Parser>::parse();
    match run(Options {
        session: cli.session,
        keys: cli.keys,
        port: cli.port,
        model: cli.model,
    }) {
        Ok(()) => {}
        Err(error) => {
            eprintln!("ui-lm-serve: {error:#}");
            std::process::exit(1);
        }
    }
}
/// The most recent page, and the document drawn from it.
#[derive(Default)]
pub struct Snapshot {
    /// Bumped whenever a new frame becomes the latest page.
    pub version: u64,
    /// The page itself, as the JSON the recorder describes.
    pub page: String,
    /// The HTML a browser shows for that page.
    pub html: String,
    /// How many frames had been read when this snapshot was taken.
    pub frames: usize,
}

impl Snapshot {
    /// The JSON a browser polls for.
    pub fn state_json(&self) -> String {
        format!(
            "{{\"version\":{},\"frames\":{},\"page\":{},\"html\":{}}}",
            self.version,
            self.frames,
            json_string(&self.page),
            json_string(&self.html)
        )
    }
}

/// A JSON string literal for `text`, escaped the way JSON requires.
fn json_string(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for character in text.chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            control if (control as u32) < 0x20 => {
                out.push_str(&format!("\\u{:04x}", control as u32));
            }
            other => out.push(other),
        }
    }
    out.push('"');
    out
}

/// What the process was asked to do.
pub struct Options {
    /// The recording to watch.
    pub session: PathBuf,
    /// The file the recorder forwards keystrokes from.
    pub keys: PathBuf,
    /// The port to listen on, on this machine only.
    pub port: u16,
    /// A trained model. When given, the pages served are the model's own rather than the
    /// rule-based reader's.
    pub model: Option<PathBuf>,
}

/// Runs the viewer until the process is stopped.
pub fn run(options: Options) -> Result<()> {
    let state = Arc::new(Mutex::new(Snapshot::default()));
    // The model is loaded once, at start: a model that failed to load must stop the viewer
    // rather than quietly serve pages from the rule-based reader as if nothing happened.
    let model = match &options.model {
        Some(path) => {
            let model = ui_lm_model::Model::load(path)?;
            println!(
                "describing screens with {} (trained on {} screen(s))",
                path.display(),
                model.trained_on
            );
            Some(Arc::new(model))
        }
        None => None,
    };
    {
        let session = options.session.clone();
        let state = Arc::clone(&state);
        let model = model.clone();
        std::thread::spawn(move || watch(&session, &state, model));
    }

    let address = SocketAddrV4::new(Ipv4Addr::LOCALHOST, options.port);
    let listener = TcpListener::bind(address)
        .with_context(|| format!("cannot listen on {address} — is the port already in use?"))?;
    println!("live view: http://{address}/");
    println!(
        "typing goes to the recording's control file: {}",
        options.keys.display()
    );

    for stream in listener.incoming() {
        let Ok(stream) = stream else { continue };
        let state = Arc::clone(&state);
        let keys = options.keys.clone();
        std::thread::spawn(move || {
            // A browser that goes away mid-request is not an error worth reporting.
            let _ = handle(stream, &state, &keys);
        });
    }
    Ok(())
}

/// The text of a screen, one row per line.
///
/// The model reads rows of text, not cells, so the frame is rendered to text first — which
/// is what a person would see if they copied the screen out of their terminal.
fn text_of(frame: &ui_lm_frames::Frame) -> String {
    let grid = ui_lm_frames::decode_grid(&frame.runs, frame.rows, frame.cols);
    grid.iter()
        .map(|cells| {
            cells
                .iter()
                .map(|cell| cell.ch)
                .collect::<String>()
                .trim_end()
                .to_owned()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Re-reads the session and refreshes the snapshot whenever a frame appears.
fn watch(session: &Path, state: &Arc<Mutex<Snapshot>>, model: Option<Arc<ui_lm_model::Model>>) {
    let mut last = usize::MAX;
    loop {
        if let Some((count, page, html)) = latest(session, model.as_deref()) {
            if count != last {
                last = count;
                let mut snapshot = state.lock().expect("the snapshot lock is never poisoned");
                snapshot.version += 1;
                snapshot.frames = count;
                snapshot.page = page;
                snapshot.html = html;
            }
        }
        std::thread::sleep(Duration::from_millis(200));
    }
}

/// The newest page for a session, its JSON, and its document.
fn latest(session: &Path, model: Option<&ui_lm_model::Model>) -> Option<(usize, String, String)> {
    let frames = match ui_lm_frames::read_frames(session) {
        Ok((_, frames)) => frames,
        Err(_) => ui_lm_frames::read_orphaned_frames(session).ok()?,
    };
    if frames.is_empty() {
        return None;
    }
    let title = session
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "session".to_owned());
    // With a model the page is the model's reading of the screen's text; without one it is
    // the rule-based reader's. Both go through the same builder, so what is served differs
    // only in judgement.
    let frame = frames.last()?;
    let page = match model {
        Some(model) => {
            // Styling comes from the cells, judgement from the model: without the graft the
            // live view of a model page had no colours at all.
            let mut page = model.page(&text_of(frame), &title);
            ui_lm_pages::graft_spans(&mut page, &ui_lm_pages::page_from_frame(frame, &title));
            page
        }
        None => ui_lm_pages::page_from_frame(frame, &title),
    };
    let json = serde_json::to_string(&page).unwrap_or_default();
    let html = ui_lm_render::render_page(&page, &title);
    Some((frames.len(), json, html))
}

/// Answers one request.
fn handle(mut stream: TcpStream, state: &Arc<Mutex<Snapshot>>, keys: &Path) -> Result<()> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut request_line = String::new();
    reader.read_line(&mut request_line)?;
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or_default().to_owned();
    let path = parts.next().unwrap_or("/").to_owned();

    // Headers, for the content length of a typed message.
    let mut length = 0usize;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line)? == 0 {
            break;
        }
        if line.trim().is_empty() {
            break;
        }
        if let Some(value) = line
            .split_once(':')
            .filter(|(name, _)| name.eq_ignore_ascii_case("content-length"))
            .map(|(_, value)| value.trim())
        {
            length = value.parse().unwrap_or(0);
        }
    }

    let body = if method == "POST" && length > 0 {
        let mut body = vec![0u8; length.min(64 * 1024)];
        reader.read_exact(&mut body)?;
        body
    } else {
        Vec::new()
    };

    // A binding arrives on the query string (`/keys?binding=Ctrl%2BN`) because a URL is
    // what a button can carry without a template; a typed message arrives as the body.
    let (route, query) = path
        .split_once('?')
        .map_or((path.as_str(), ""), |(route, query)| (route, query));
    let binding = query
        .split('&')
        .filter_map(|pair| pair.split_once('='))
        .find(|(name, _)| *name == "binding")
        .and_then(|(_, value)| decode_percent(value));

    match (method.as_str(), route) {
        ("GET", "/") => {
            let html = state
                .lock()
                .expect("the snapshot lock is never poisoned")
                .html
                .clone();
            respond(
                &mut stream,
                "200 OK",
                "text/html; charset=utf-8",
                shell(&html).as_bytes(),
            )
        }
        ("GET", "/state") => {
            let json = state
                .lock()
                .expect("the snapshot lock is never poisoned")
                .state_json();
            respond(&mut stream, "200 OK", "application/json", json.as_bytes())
        }
        ("POST", "/keys") if binding.is_some() => {
            let binding = binding.unwrap_or_default();
            let Some(bytes) = keys::keystrokes(&binding) else {
                return respond(
                    &mut stream,
                    "400 Bad Request",
                    "text/plain; charset=utf-8",
                    b"this page names a binding this viewer cannot send",
                );
            };
            let mut file = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(keys)
                .with_context(|| format!("cannot open the control file {}", keys.display()))?;
            file.write_all(&bytes)?;
            file.flush()?;
            respond(&mut stream, "204 No Content", "text/plain", b"")
        }
        ("POST", "/keys") => {
            // Appending is the whole protocol: the recorder reads what is new.
            let mut file = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(keys)
                .with_context(|| format!("cannot open the control file {}", keys.display()))?;
            file.write_all(&body)?;
            file.flush()?;
            respond(&mut stream, "204 No Content", "text/plain", b"")
        }
        _ => respond(
            &mut stream,
            "404 Not Found",
            "text/plain; charset=utf-8",
            b"not found",
        ),
    }
}

/// Writes one response and closes the connection.
fn respond(stream: &mut TcpStream, status: &str, kind: &str, body: &[u8]) -> Result<()> {
    let header = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {kind}\r\nContent-Length: {}\r\n\
         Cache-Control: no-store\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream.write_all(header.as_bytes())?;
    stream.write_all(body)?;
    stream.flush()?;
    Ok(())
}

/// The page a browser loads: the drawn screen, refreshed, with somewhere to type.
fn shell(html: &str) -> String {
    // The drawn page is inserted as the initial content and then replaced wholesale from
    // `/state`, so the browser never needs a reload and the server never needs a template.
    format!(
        r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>UI-LM live view</title>
<style>
body {{ margin: 0; background: #0b0b0c; color: #d6d6d7;
        font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace; }}
#live {{ padding: 16px 16px 0; }}
#type {{ position: sticky; bottom: 0; display: flex; gap: 8px; padding: 10px 16px;
         background: #101013; border-top: 1px solid #2a2a31; }}
#type input {{ flex: 1; background: #0b0b0c; color: #d6d6d7; border: 1px solid #33333a;
               border-radius: 6px; padding: 8px 10px; font: inherit; }}
#type button {{ background: #1d3a2a; color: #b7e4c7; border: 1px solid #2f5a40;
                border-radius: 6px; padding: 8px 14px; font: inherit; cursor: pointer; }}
#status {{ color: #6f6f76; font-size: 11px; padding: 0 16px 8px; }}
</style>
</head>
<body>
<div id="live">{html}</div>
<div id="status">connecting…</div>
<form id="type"><input id="keys" placeholder="type here and press Enter — it goes to the program" autocomplete="off"><button>send</button></form>
<script>
let version = -1;
async function poll() {{
  try {{
    const response = await fetch('/state', {{cache: 'no-store'}});
    const state = await response.json();
    if (state.version !== version) {{
      version = state.version;
      document.getElementById('live').innerHTML = state.html;
    }}
    document.getElementById('status').textContent =
      'live — ' + state.frames + ' frame(s) recorded, updating';
  }} catch (error) {{
    document.getElementById('status').textContent = 'not connected: ' + error;
  }}
  setTimeout(poll, 400);
}}
document.getElementById('type').addEventListener('submit', async (event) => {{
  event.preventDefault();
  const field = document.getElementById('keys');
  await fetch('/keys', {{method: 'POST', body: field.value + '\r'}});
  field.value = '';
}});
document.addEventListener('click', async (event) => {{
  const button = event.target.closest('[data-binding]');
  if (!button) {{ return; }}
  event.preventDefault();
  const binding = button.dataset.binding;
  const response = await fetch('/keys?binding=' + encodeURIComponent(binding), {{method: 'POST'}});
  document.getElementById('status').textContent = response.ok
    ? 'sent ' + binding
    : 'this page cannot send ' + binding;
}});
poll();
</script>
</body>
</html>
"#
    )
}

/// A percent-decoded query value.
///
/// `+` is kept as itself rather than read as a space: every binding this page sends is
/// encoded by the browser's `encodeURIComponent`, which encodes a real space as `%20` and
/// leaves the `+` of `Ctrl+N` to be encoded as `%2B`. Reading a raw `+` as a space would
/// turn `Ctrl+N` into `Ctrl N` for any client that encodes it the other way.
fn decode_percent(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'%' => {
                let hex = text.get(index + 1..index + 3)?;
                out.push(u8::from_str_radix(hex, 16).ok()?);
                index += 3;
            }
            byte => {
                out.push(byte);
                index += 1;
            }
        }
    }
    String::from_utf8(out).ok()
}

/// The port a URL names, for the tests.
pub fn parse_port(text: &str) -> Option<u16> {
    text.trim().parse().ok()
}

/// The latest page as JSON, for callers that want it without a browser.
pub fn page_json(page: &Envelope) -> String {
    serde_json::to_string(page).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_strings_break_on_quotes_and_newlines() {
        assert_eq!(json_string("a\"b"), "\"a\\\"b\"");
        assert_eq!(json_string("a\nb"), "\"a\\nb\"");
        assert_eq!(json_string("a\\b"), "\"a\\\\b\"");
    }

    #[test]
    fn the_state_document_is_valid_json_with_the_page_in_it() {
        let snapshot = Snapshot {
            version: 3,
            frames: 7,
            page: "{\"a\":1}".to_owned(),
            html: "<b>x</b>".to_owned(),
        };
        let json = snapshot.state_json();
        assert!(json.starts_with("{\"version\":3,\"frames\":7,"), "{json}");
        assert!(
            json.contains("\\\"a\\\":1"),
            "the page must be embedded: {json}"
        );
    }

    #[test]
    fn a_request_is_answered_and_the_connection_closes() {
        // A real socket pair would be better than a stub, but the parts worth testing are
        // the routing and the response shape, and both are exercised here through the
        // same handler the listener uses.
        let state = Arc::new(Mutex::new(Snapshot {
            version: 1,
            frames: 1,
            page: "{}".to_owned(),
            html: "<i>screen</i>".to_owned(),
        }));
        let dir = tempfile::tempdir().unwrap();
        let keys = dir.path().join("keys.txt");

        let listener = TcpListener::bind(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 0)).unwrap();
        let address = listener.local_addr().unwrap();
        let handle_state = Arc::clone(&state);
        let handle_keys = keys.clone();
        let server = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            handle(stream, &handle_state, &handle_keys).unwrap();
        });

        let mut client = TcpStream::connect(address).unwrap();
        client
            .write_all(b"GET /state HTTP/1.1\r\nHost: localhost\r\n\r\n")
            .unwrap();
        let mut response = String::new();
        client.read_to_string(&mut response).unwrap();
        server.join().unwrap();

        assert!(response.starts_with("HTTP/1.1 200 OK"), "{response}");
        assert!(response.contains("\"frames\":1"), "{response}");
    }

    #[test]
    fn a_typed_message_reaches_the_control_file() {
        let state = Arc::new(Mutex::new(Snapshot::default()));
        let dir = tempfile::tempdir().unwrap();
        let keys = dir.path().join("keys.txt");

        let listener = TcpListener::bind(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 0)).unwrap();
        let address = listener.local_addr().unwrap();
        let handle_state = Arc::clone(&state);
        let handle_keys = keys.clone();
        let server = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            handle(stream, &handle_state, &handle_keys).unwrap();
        });

        let body = "hello\r";
        let mut client = TcpStream::connect(address).unwrap();
        write!(
            client,
            "POST /keys HTTP/1.1\r\nHost: localhost\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        )
        .unwrap();
        let mut response = String::new();
        client.read_to_string(&mut response).unwrap();
        server.join().unwrap();

        assert!(response.starts_with("HTTP/1.1 204"), "{response}");
        assert_eq!(std::fs::read(&keys).unwrap(), b"hello\r");
    }

    #[test]
    fn a_clicked_binding_becomes_the_bytes_of_its_key() {
        // What the browser does when a button on the page is pressed: one POST naming the
        // binding, and the program receives that key.
        let state = Arc::new(Mutex::new(Snapshot::default()));
        let dir = tempfile::tempdir().unwrap();
        let keys = dir.path().join("keys.txt");

        let listener = TcpListener::bind(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 0)).unwrap();
        let address = listener.local_addr().unwrap();
        let handle_state = Arc::clone(&state);
        let handle_keys = keys.clone();
        let server = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            handle(stream, &handle_state, &handle_keys).unwrap();
        });

        let mut client = TcpStream::connect(address).unwrap();
        write!(
            client,
            "POST /keys?binding=Ctrl%2BN HTTP/1.1\r\nHost: localhost\r\n\
             Content-Length: 0\r\n\r\n"
        )
        .unwrap();
        let mut response = String::new();
        client.read_to_string(&mut response).unwrap();
        server.join().unwrap();

        assert!(response.starts_with("HTTP/1.1 204"), "{response}");
        assert_eq!(std::fs::read(&keys).unwrap(), vec![0x0e], "Ctrl+N is 0x0E");
    }

    #[test]
    fn a_binding_the_viewer_cannot_send_is_refused() {
        let state = Arc::new(Mutex::new(Snapshot::default()));
        let dir = tempfile::tempdir().unwrap();
        let keys = dir.path().join("keys.txt");

        let listener = TcpListener::bind(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 0)).unwrap();
        let address = listener.local_addr().unwrap();
        let handle_state = Arc::clone(&state);
        let handle_keys = keys.clone();
        let server = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            handle(stream, &handle_state, &handle_keys).unwrap();
        });

        let mut client = TcpStream::connect(address).unwrap();
        write!(
            client,
            "POST /keys?binding=F7 HTTP/1.1\r\nHost: localhost\r\nContent-Length: 0\r\n\r\n"
        )
        .unwrap();
        let mut response = String::new();
        client.read_to_string(&mut response).unwrap();
        server.join().unwrap();

        assert!(response.starts_with("HTTP/1.1 400"), "{response}");
        assert!(
            !keys.exists() || std::fs::read(&keys).unwrap().is_empty(),
            "nothing may be typed when the binding is not understood"
        );
    }

    #[test]
    fn a_percent_encoded_binding_decodes_and_a_plus_stays_a_plus() {
        assert_eq!(decode_percent("Ctrl%2BN").as_deref(), Some("Ctrl+N"));
        assert_eq!(decode_percent("Ctrl+N").as_deref(), Some("Ctrl+N"));
        assert_eq!(decode_percent("a%20b").as_deref(), Some("a b"));
        assert_eq!(decode_percent("%zz"), None);
    }
}
