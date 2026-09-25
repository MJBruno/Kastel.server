use std::io::{self, BufRead, BufReader, Read, Write};

use serde_json::Value;

mod analyzer;
mod class_index;
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

    fn read_message(&mut self) -> io::Result<Option<Value>> {
        let mut content_length = None;

        loop {
            let mut line = String::new();

            let bytes = self.reader.read_line(&mut line)?;

            if bytes == 0 {
                return Ok(None);
            }

            if line == "\r\n" {
                break;
            }

            if let Some(value) = line.strip_prefix("Content-Length:") {
                content_length = Some(value.trim().parse::<usize>().map_err(|_| {
                    io::Error::new(io::ErrorKind::InvalidData, "invalid Content-Length")
                })?);
            }
        }

        let length = content_length
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing Content-Length"))?;

        let mut body = vec![0u8; length];

        self.reader.read_exact(&mut body)?;

        let message = serde_json::from_slice(&body).map_err(io::Error::other)?;

        Ok(Some(message))
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
        eprintln!("Received LSP message");

        let request: RpcRequest = serde_json::from_value(value).map_err(io::Error::other)?;

        let messages = server.handle(request);

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
