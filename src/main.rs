use std::io::{self, BufRead, BufReader, Read, Write};
use std::panic::{self, AssertUnwindSafe};

use serde_json::Value;

mod analyzer;
mod class_index;
mod code_actions;
mod completion;
mod definition;
mod diagnostics;
mod document_highlight;
mod document_protocol;
mod formatting;
mod hover;
mod language;
mod lsp_position;
mod module_resolver;
mod position;
mod protocol;
mod references;
mod rename;
mod semantic;
mod server;
mod signature_help;
mod source_position;
mod span;
mod symbols;
mod text_util;
mod type_info;
mod uri_util;
mod workspace;
mod workspace_index; // ← AJOUT

use protocol::{RpcRequest, RpcResponse};
use server::{Server, ServerMessage};

struct Transport {
    reader: BufReader<io::Stdin>,
    stdout: io::Stdout,
}

impl Transport {
    fn new() -> Self {
        Self {
            reader: BufReader::new(io::stdin()),
            stdout: io::stdout(),
        }
    }

    /// Lit un message LSP (`Content-Length` puis corps JSON).
    ///
    /// Tolérant aux fins de ligne `\n` seules, à la casse de l'en-tête et aux
    /// en-têtes supplémentaires (`Content-Type`). Un corps JSON invalide est
    /// signalé sur stderr et ignoré (`Value::Null`) au lieu d'arrêter le serveur.
    fn read_message(&mut self) -> io::Result<Option<Value>> {
        let mut content_length = None;

        loop {
            let mut line = String::new();

            let bytes = self.reader.read_line(&mut line)?;

            if bytes == 0 {
                return Ok(None);
            }

            let trimmed = line.trim();

            if trimmed.is_empty() {
                if content_length.is_some() {
                    break;
                }

                continue;
            }

            if let Some((name, value)) = trimmed.split_once(':') {
                if name.trim().eq_ignore_ascii_case("content-length") {
                    content_length = Some(value.trim().parse::<usize>().map_err(|_| {
                        io::Error::new(io::ErrorKind::InvalidData, "invalid Content-Length")
                    })?);
                }
            }
        }

        let length = content_length
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing Content-Length"))?;

        let mut body = vec![0u8; length];

        self.reader.read_exact(&mut body)?;

        match serde_json::from_slice(&body) {
            Ok(message) => Ok(Some(message)),
            Err(error) => {
                eprintln!("Invalid JSON message ignored: {error}");
                Ok(Some(Value::Null))
            }
        }
    }

    fn send_message(&mut self, message: &RpcResponse) -> io::Result<()> {
        let body = serde_json::to_vec(message).map_err(io::Error::other)?;

        self.write_body(&body)
    }

    fn send_value(&mut self, message: &Value) -> io::Result<()> {
        let body = serde_json::to_vec(message).map_err(io::Error::other)?;

        self.write_body(&body)
    }

    fn write_body(&mut self, body: &[u8]) -> io::Result<()> {
        write!(self.stdout, "Content-Length: {}\r\n\r\n", body.len())?;

        self.stdout.write_all(body)?;
        self.stdout.flush()?;

        Ok(())
    }
}

fn main() -> io::Result<()> {
    eprintln!("[FORGE]: Kastel Server starting...");

    let mut transport = Transport::new();
    let mut server = Server::new();

    while let Some(value) = transport.read_message()? {
        // Pas de champ "method" : réponse du client à une requête du serveur
        // (ou message invalide). Rien à traiter.
        if value.get("method").and_then(Value::as_str).is_none() {
            continue;
        }

        let request: RpcRequest = match serde_json::from_value(value) {
            Ok(request) => request,
            Err(error) => {
                eprintln!("Invalid LSP request ignored: {error}");
                continue;
            }
        };

        let request_id = request.id.clone();
        let method = request.method.clone();

        // Une panique dans l'analyse d'un document ne doit pas tuer le serveur.
        let outcome = panic::catch_unwind(AssertUnwindSafe(|| server.handle(request)));

        let messages = match outcome {
            Ok(messages) => messages,
            Err(_) => {
                eprintln!("Internal error while handling '{method}'");

                match request_id {
                    Some(id) => vec![ServerMessage::Response(RpcResponse::error(
                        id,
                        -32603,
                        format!("Internal error while handling '{method}'"),
                    ))],
                    None => Vec::new(),
                }
            }
        };

        for message in messages {
            match message {
                ServerMessage::Response(response) => {
                    transport.send_message(&response)?;
                }

                ServerMessage::Notification(notification) => {
                    transport.send_value(&notification)?;
                }
            }
        }

        if server.is_shutdown() {
            break;
        }
    }

    eprintln!("[FORGE]: Kastel Server stopped");

    Ok(())
}
