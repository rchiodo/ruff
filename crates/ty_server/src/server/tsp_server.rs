//! TSP (Type Server Protocol) server implementation.
//!
//! This module provides a standalone `TspServer` that wraps an inner LSP server
//! and handles Type Server Protocol (TSP) requests in addition to Language Server
//! Protocol (LSP) requests.
//!
//! ## Architecture
//!
//! The `TspServer` is a wrapper around the standard LSP `Server` that:
//! 1. **Request Routing**: Routes requests based on method name prefix
//!    - `typeServer/*` methods → TSP handlers
//!    - All other methods → LSP handlers (delegated to inner server)
//! 2. **Delegation**: Uses the inner server's session, client, and scheduling infrastructure
//! 3. **Compatibility**: Maintains full LSP compatibility while adding TSP support
//!
//! ## Usage
//!
//! ```rust,no_run
//! use ty_server::server::{Server, TspServer};
//!
//! // Create TSP server wrapping LSP server
//! let lsp_server = Server::new(worker_threads, connection, system, false)?;
//! let tsp_server = TspServer::new(lsp_server);
//!
//! // Run TSP server (handles both TSP and LSP)
//! tsp_server.run()?;
//! ```
//!
//! ## TSP Request Handling
//!
//! TSP requests are handled by dedicated handlers in the `tsp_api` module:
//! - `typeServer/getType` → `GetTypeRequestHandler`
//! - `typeServer/getTypeArgs` → `GetTypeArgsRequestHandler`
//! - `typeServer/getSupportedProtocolVersion` → Returns protocol version
//! - Future TSP methods can be easily added
//!
//! ## Message Flow
//!
//! ```text
//! Client Request
//!      ↓
//! TspServer::run()
//!      ↓
//! ┌─────────────────┐    ┌─────────────────┐
//! │ TSP Handler     │    │ Inner LSP       │
//! │ (typeServer/*) │    │ Server          │
//! │                 │    │ (everything     │
//! │                 │    │ else)           │
//! └─────────────────┘    └─────────────────┘
//!      ↓                          ↓
//! TSP Response              LSP Response
//! ```

use crate::server::schedule::Scheduler;
use crate::server::{Server, api};
use crate::session::client::Client;
use anyhow::anyhow;
use lsp_server::Message;
use lsp_types::notification::Notification;

use super::{Action, Event};

/// A TSP server that wraps an inner LSP server and handles both TSP and LSP requests.
pub struct TspServer {
    /// The inner LSP server that handles standard LSP requests
    inner: Server,
    /// The current revision number, updated when global state changes
    current_revision: u64,
}

impl TspServer {
    /// Create a new TSP server wrapping the given LSP server.
    pub fn new(inner: Server) -> Self {
        Self {
            inner,
            current_revision: 0,
        }
    }

    /// Run the TSP server main loop, handling both TSP and LSP messages.
    pub fn run(mut self) -> crate::Result<()> {
        let client = Client::new(
            self.inner.main_loop_sender.clone(),
            self.inner.connection.sender.clone(),
        );

        let _panic_hook = super::ServerPanicHookHandler::new(client);

        crate::server::schedule::spawn_main_loop(move || self.main_loop())?.join()
    }

    /// Check if a request method is a TSP request.
    pub fn is_tsp_request(method: &str) -> bool {
        method.starts_with("typeServer/")
    }

    /// TSP-aware main loop that handles both TSP and LSP messages.
    fn main_loop(&mut self) -> crate::Result<()> {
        self.inner.initialize(&Client::new(
            self.inner.main_loop_sender.clone(),
            self.inner.connection.sender.clone(),
        ));

        let mut scheduler = Scheduler::new(self.inner.worker_threads);

        while let Ok(next_event) = self.inner.next_event() {
            let Some(next_event) = next_event else {
                anyhow::bail!("client exited without proper shutdown sequence");
            };

            let client = Client::new(
                self.inner.main_loop_sender.clone(),
                self.inner.connection.sender.clone(),
            );

            match next_event {
                Event::Message(msg) => {
                    let Some(msg) = self.inner.session.should_defer_message(msg) else {
                        continue;
                    };

                    let task = match msg {
                        Message::Request(req) => {
                            self.inner
                                .session
                                .request_queue_mut()
                                .incoming_mut()
                                .register(req.id.clone(), req.method.clone());

                            if self.inner.session.is_shutdown_requested() {
                                tracing::warn!(
                                    "Received request `{}` after server shutdown was requested, discarding",
                                    &req.method
                                );
                                client.respond_err(
                                    req.id,
                                    lsp_server::ResponseError {
                                        code: lsp_server::ErrorCode::InvalidRequest as i32,
                                        message: "Shutdown already requested".to_owned(),
                                        data: None,
                                    },
                                );
                                continue;
                            }

                            // Route TSP requests to TSP handler, LSP requests to LSP handler
                            if Self::is_tsp_request(&req.method) {
                                tsp_api::request(req, self.current_revision)
                            } else {
                                api::request(req)
                            }
                        }
                        Message::Notification(notification) => {
                            if notification.method == lsp_types::notification::Exit::METHOD {
                                if !self.inner.session.is_shutdown_requested() {
                                    return Err(anyhow!(
                                        "Received exit notification before a shutdown request"
                                    ));
                                }

                                tracing::debug!("Received exit notification, exiting");
                                return Ok(());
                            }

                            // TSP notifications would be handled here if needed
                            // For now, delegate all notifications to LSP handler
                            api::notification(notification)
                        }

                        // Handle the response from the client to a server request
                        Message::Response(response) => {
                            if let Some(handler) = self
                                .inner
                                .session
                                .request_queue_mut()
                                .outgoing_mut()
                                .complete(&response.id)
                            {
                                handler.handle_response(&client, response);
                            } else {
                                tracing::error!(
                                    "Received a response with ID {}, which was not expected",
                                    response.id
                                );
                            }

                            continue;
                        }
                    };

                    scheduler.dispatch(task, &mut self.inner.session, client);
                }
                Event::Action(action) => match action {
                    Action::SendResponse(response) => {
                        // Filter out responses for already canceled requests.
                        if let Some((start_time, method)) = self
                            .inner
                            .session
                            .request_queue_mut()
                            .incoming_mut()
                            .complete(&response.id)
                        {
                            let duration = start_time.elapsed();
                            tracing::trace!(name: "message response", method, %response.id, duration = format_args!("{:0.2?}", duration));

                            self.inner
                                .connection
                                .sender
                                .send(Message::Response(response))?;
                        } else {
                            tracing::trace!(
                                "Ignoring response for canceled request id={}",
                                response.id
                            );
                        }
                    }

                    Action::RetryRequest(request) => {
                        // Never retry canceled requests.
                        if self
                            .inner
                            .session
                            .request_queue()
                            .incoming()
                            .is_pending(&request.id)
                        {
                            let task = if Self::is_tsp_request(&request.method) {
                                tsp_api::request(request, self.current_revision)
                            } else {
                                api::request(request)
                            };
                            scheduler.dispatch(task, &mut self.inner.session, client);
                        } else {
                            tracing::debug!(
                                "Request {}/{} was cancelled, not retrying",
                                request.method,
                                request.id
                            );
                        }
                    }

                    Action::SendRequest(request) => {
                        client.send_request_raw(&self.inner.session, request)
                    }

                    Action::SuspendWorkspaceDiagnostics(suspended_request) => {
                        self.inner
                            .session
                            .set_suspended_workspace_diagnostics_request(
                                *suspended_request,
                                &client,
                            );
                    }

                    Action::InitializeWorkspaces(workspaces_with_options) => {
                        self.inner
                            .session
                            .initialize_workspaces(workspaces_with_options, &client);
                        // We do this here after workspaces have been initialized
                        // so that the file watcher globs can take project search
                        // paths into account.
                        // self.try_register_file_watcher(&client);
                    }

                    Action::GlobalStateChanged { revision } => {
                        // Update our tracked revision
                        self.current_revision = revision;
                        // For now, just log that the global state changed in TSP server
                        // In the future, this could be used to notify TSP clients,
                        // invalidate type caches, trigger re-computation, etc.
                        tracing::debug!(
                            "TSP Server: Global state changed (revision: {})",
                            revision
                        );
                    }
                },
            }
        }

        Ok(())
    }
}

/// TSP-specific API module for handling TSP requests.
mod tsp_api {
    use crate::server::schedule::Task;
    use crate::server::tsp;
    use anyhow::anyhow;
    use lsp_server as server;
    use tsp::{GetTypeResponse, TSPRequests};

    /// Converts a `serde_json::Value` ID to `lsp_server::RequestId`.
    fn convert_request_id(id: serde_json::Value) -> Result<lsp_server::RequestId, anyhow::Error> {
        match id {
            serde_json::Value::String(s) => Ok(lsp_server::RequestId::from(s)),
            serde_json::Value::Number(n) => {
                if let Some(i) = n.as_i64() {
                    #[allow(clippy::cast_possible_truncation)]
                    Ok(lsp_server::RequestId::from(i as i32))
                } else {
                    Err(anyhow!("Invalid request ID format: number out of range"))
                }
            }
            _ => Err(anyhow!(
                "Invalid request ID format: must be string or number"
            )),
        }
    }

    /// Macro to handle request ID conversion with error handling.
    macro_rules! handle_request_id {
        ($id:expr, $orig_request_id:expr) => {
            match convert_request_id($id) {
                Ok(id) => id,
                Err(err) => {
                    let result: crate::server::Result<()> = Err(crate::server::api::Error::new(
                        err,
                        server::ErrorCode::InvalidRequest,
                    ));
                    return Task::immediate($orig_request_id, result);
                }
            }
        };
    }

    /// Processes a TSP request from the client to the server.
    pub(super) fn request(req: server::Request, current_revision: u64) -> Task {
        let request_id = req.id.clone();

        // Parse the entire request (method + params) as a TSP request enum
        let tsp_request = match serde_json::from_value::<TSPRequests>(
            serde_json::to_value(req).unwrap_or(serde_json::Value::Null),
        ) {
            Ok(request) => request,
            Err(err) => {
                tracing::warn!("Failed to parse TSP request: {}", err);
                let result: crate::server::Result<()> = Err(crate::server::api::Error::new(
                    anyhow!("Invalid TSP request format: {}", err),
                    server::ErrorCode::ParseError,
                ));
                return Task::immediate(request_id, result);
            }
        };

        match tsp_request {
            TSPRequests::GetTypeRequest { id, params } => {
                // Convert serde_json::Value to lsp_server::RequestId
                let request_id = handle_request_id!(id, request_id);

                Task::sync(move |session, client| {
                    // Parameters are already extracted and validated by the enum deserialization
                    let url = tsp::requests::get_type::GetTypeRequestHandler::document_url(&params);
                    let snapshot = session.take_document_snapshot(url.into_owned());

                    if let Ok(document_query) = snapshot.document() {
                        let db = session.project_db(document_query.file_path());
                        tsp::requests::get_type::GetTypeRequestHandler::handle_request(
                            &request_id,
                            db,
                            &snapshot,
                            client,
                            &params,
                        );
                    } else {
                        client.respond::<GetTypeResponse>(
                            &request_id,
                            Err(crate::server::api::Error::new(
                                anyhow::anyhow!("Failed to resolve document"),
                                lsp_server::ErrorCode::InternalError,
                            )),
                        );
                    }
                })
            }

            TSPRequests::GetTypeArgsRequest { id, params } => {
                // Convert serde_json::Value to lsp_server::RequestId
                let request_id = handle_request_id!(id, request_id);

                Task::sync(move |session, client| {
                    // For getTypeArgs, we need access to any project database
                    // Since we're working with type handles, we don't need a specific document
                    if let Some(db) = session.project_dbs().next() {
                        // Create any document snapshot for the API (this is a limitation of current API)
                        // In a proper implementation, we wouldn't need a document snapshot for type handles
                        let workspace_uris = session.workspaces().urls().collect::<Vec<_>>();

                        if let Some(workspace_url) = workspace_uris.first() {
                            let doc_snapshot =
                                session.take_document_snapshot((*workspace_url).clone());

                            tsp::requests::get_type_args::GetTypeArgsRequestHandler::handle_request(
                                &request_id,
                                db,
                                &doc_snapshot,
                                client,
                                &params,
                            );
                        } else {
                            // No workspaces available - respond with error
                            client.respond::<Vec<crate::server::tsp::Type>>(
                                &request_id,
                                Err(crate::server::api::Error::new(
                                    anyhow::anyhow!("No workspaces available for getTypeArgs"),
                                    lsp_server::ErrorCode::InternalError,
                                )),
                            );
                        }
                    } else {
                        client.respond::<Vec<crate::server::tsp::Type>>(
                            &request_id,
                            Err(crate::server::api::Error::new(
                                anyhow::anyhow!("No project database available"),
                                lsp_server::ErrorCode::InternalError,
                            )),
                        );
                    }
                })
            }

            TSPRequests::GetSupportedProtocolVersionRequest { id } => {
                // Convert serde_json::Value to lsp_server::RequestId
                let request_id = handle_request_id!(id, request_id);

                // Return the protocol version immediately
                let result = Ok(tsp::TYPE_SERVER_VERSION.to_string());
                Task::immediate(request_id, result)
            }

            TSPRequests::GetSnapshotRequest { id } => {
                // Convert serde_json::Value to lsp_server::RequestId
                let request_id = handle_request_id!(id, request_id);

                // Return the current revision as the snapshot version
                #[allow(clippy::cast_possible_truncation)]
                let result = Ok(current_revision as i32);
                Task::immediate(request_id, result)
            }

            _ => {
                tracing::warn!(
                    "Received TSP request {:?} which does not have a handler",
                    tsp_request
                );
                let result: crate::server::Result<()> = Err(crate::server::api::Error::new(
                    anyhow!("Unimplemented TSP request: {:?}", tsp_request),
                    server::ErrorCode::MethodNotFound,
                ));
                Task::immediate(request_id, result)
            }
        }
    }
}
