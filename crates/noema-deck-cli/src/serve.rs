//! The map served to a browser on this machine.
//!
//! A pane can type into an agent, so requests are held to two rules. The page
//! must be this server's own, which a page reached under another name is not.
//! And a request must be JSON, which a page from another site cannot send here
//! without asking first, and this server never grants that.

use std::{
    fs,
    io::{BufRead, BufReader, Read, Write},
    net::{TcpListener, TcpStream},
    path::{Path, PathBuf},
    sync::Arc,
    thread,
    time::Duration,
};

use anyhow::{Context, Result, ensure};
use noema_deck_core::{Deck, usage::LimitsRequest};
use serde_json::json;

/// The largest request body read. Requests are small; pasted text is the most.
const BODY_LIMIT: usize = 1 << 20;

/// Serves the interface in `ui` and answers its requests until stopped.
pub fn run(deck: Deck, ui: &Path, port: u16) -> Result<()> {
    ensure!(ui.join("index.html").is_file(), "No interface in {}", ui.display());
    let listener =
        TcpListener::bind(("127.0.0.1", port)).with_context(|| format!("Listen on port {port}"))?;
    println!("Serving the map at http://127.0.0.1:{port}");
    let server = Arc::new(Server { deck, ui: ui.into(), port });
    for stream in listener.incoming() {
        let server = Arc::clone(&server);
        // A read waits for a pane's output, so each connection gets a thread.
        thread::spawn(move || {
            if let Err(error) = stream.map_err(Into::into).and_then(|stream| server.serve(stream)) {
                eprintln!("{error:#}");
            }
        });
    }
    Ok(())
}

struct Server {
    deck: Deck,
    ui: PathBuf,
    port: u16,
}

/// The parts of a request this server reads.
struct Request {
    method: String,
    path: String,
    host: String,
    is_json: bool,
    body: Vec<u8>,
}

struct Response {
    status: &'static str,
    kind: &'static str,
    body: Vec<u8>,
}

impl Response {
    fn text(status: &'static str) -> Self {
        Self { status, kind: "text/plain", body: status.into() }
    }
}

impl Server {
    fn serve(&self, mut stream: TcpStream) -> Result<()> {
        stream.set_read_timeout(Some(Duration::from_secs(2)))?;
        let response = self.answer(&read(&stream)?);
        write!(
            stream,
            "HTTP/1.1 {}\r\nContent-Type: {}\r\nContent-Length: {}\r\n\
             Cache-Control: no-store\r\nConnection: close\r\n\r\n",
            response.status,
            response.kind,
            response.body.len()
        )?;
        stream.write_all(&response.body)?;
        Ok(())
    }

    fn answer(&self, request: &Request) -> Response {
        let own = [format!("127.0.0.1:{}", self.port), format!("localhost:{}", self.port)];
        if !own.contains(&request.host) {
            return Response::text("403 Forbidden");
        }
        match (request.method.as_str(), request.path.as_str()) {
            ("POST", route @ ("/map" | "/pane" | "/usage" | "/limits" | "/fonts"))
                if request.is_json =>
            {
                match self.call(route, &request.body) {
                    Ok(body) => Response { status: "200 OK", kind: "application/json", body },
                    Err(error) => {
                        let body = json!({ "error": format!("{error:#}") }).to_string().into();
                        Response { status: "400 Bad Request", kind: "application/json", body }
                    }
                }
            }
            ("GET", path) if !path.contains("..") => self.file(path),
            _ => Response::text("400 Bad Request"),
        }
    }

    fn call(&self, route: &str, body: &[u8]) -> Result<Vec<u8>> {
        let invalid = "Invalid request";
        let answer = if route == "/fonts" {
            serde_json::to_vec(noema_deck_core::fonts::installed())
        } else if route == "/usage" {
            serde_json::to_vec(&self.deck.usage()?)
        } else if route == "/limits" {
            let request: LimitsRequest = serde_json::from_slice(body).context(invalid)?;
            serde_json::to_vec(&self.deck.limits(request)?)
        } else if route == "/pane" {
            serde_json::to_vec(&self.deck.pane(serde_json::from_slice(body).context(invalid)?)?)
        } else {
            serde_json::to_vec(&self.deck.map(serde_json::from_slice(body).context(invalid)?)?)
        };
        Ok(answer?)
    }

    fn file(&self, path: &str) -> Response {
        let name = if path == "/" { "index.html" } else { path.trim_start_matches('/') };
        let kind = match Path::new(name).extension().and_then(|extension| extension.to_str()) {
            Some("html") => "text/html; charset=utf-8",
            Some("css") => "text/css",
            Some("js") => "text/javascript",
            Some("svg") => "image/svg+xml",
            Some("json") => "application/json",
            _ => "application/octet-stream",
        };
        match fs::read(self.ui.join(name)) {
            Ok(body) => Response { status: "200 OK", kind, body },
            Err(_) => Response::text("404 Not Found"),
        }
    }
}

fn read(stream: &TcpStream) -> Result<Request> {
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    reader.read_line(&mut line)?;
    let mut words = line.split_whitespace().map(String::from);
    let (method, target) = (words.next().unwrap_or_default(), words.next().unwrap_or_default());
    let path = target.split('?').next().unwrap_or_default().to_owned();

    let (mut host, mut is_json, mut length) = (String::new(), false, 0);
    loop {
        line.clear();
        reader.read_line(&mut line)?;
        let Some((name, value)) = line.trim_end().split_once(':') else {
            break;
        };
        let value = value.trim();
        match name.to_ascii_lowercase().as_str() {
            "host" => value.clone_into(&mut host),
            "content-type" => is_json = value.starts_with("application/json"),
            "content-length" => length = value.parse()?,
            _ => {}
        }
    }
    ensure!(length <= BODY_LIMIT, "The request is too large");
    let mut body = vec![0; length];
    reader.read_exact(&mut body)?;
    Ok(Request { method, path, host, is_json, body })
}
