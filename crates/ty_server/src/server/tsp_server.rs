//! TSP (Type Server Protocol) server implementation that delegates to LSP server.
//!
//! This module provides a `TspServer` trait that extends the standard LSP server
//! to handle Type Server Protocol (TSP) requests in addition to Language Server
//! Protocol (LSP) requests.
//!
//! ## Architecture
//!
//! The TSP server wraps the existing LSP server and:
//! 1. **Request Routing**: Routes requests based on method name prefix
//!    - `typeServer/*` methods → TSP handlers
//!    - All other methods → LSP handlers
//! 2. **Delegation**: Uses the same session, client, and scheduling infrastructure
//! 3. **Compatibility**: Maintains full LSP compatibility while adding TSP support
//!
//! ## Usage
//!
//! ```rust,no_run
//! use ty_server::server::{Server, TspServer};
//!
//! // Create server as usual
//! let server = Server::new(worker_threads, connection, system, false)?;
//!
//! // Run with TSP support (handles both TSP and LSP)
//! server.run_tsp()?;
//!
//! // vs standard LSP-only mode:
//! // server.run()?;
//! ```
//!
//! ## TSP Request Handling
//!
//! TSP requests are handled by dedicated handlers in the `tsp_api` module:
//! - `typeServer/getType` → `GetTypeRequestHandler`
//! - Future TSP methods can be easily added
//!
//! ## Message Flow
//!
//! ```text
//! Client Request
//!      ↓
//! TSP Main Loop
//!      ↓
//! ┌─────────────────┐    ┌─────────────────┐
//! │ TSP Handler     │    │ LSP Handler     │
//! │ (typeServer/*) │    │ (everything     │
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

/// A trait for servers that can handle TSP (Type Server Protocol) requests.
pub trait TspServer {
    /// Run the TSP server main loop, handling both TSP and LSP messages.
    fn run_tsp(self) -> crate::Result<()>;

    /// Check if a request method is a TSP request.
    fn is_tsp_request(method: &str) -> bool;
}

impl TspServer for Server {
    fn run_tsp(mut self) -> crate::Result<()> {
        let client = Client::new(
            self.main_loop_sender.clone(),
            self.connection.sender.clone(),
        );

        let _panic_hook = super::ServerPanicHookHandler::new(client);

        crate::server::schedule::spawn_main_loop(move || self.tsp_main_loop())?.join()
    }

    fn is_tsp_request(method: &str) -> bool {
        method.starts_with("typeServer/")
    }
}

impl Server {
    /// Check if a request method is a TSP request.
    pub fn is_tsp_request(method: &str) -> bool {
        method.starts_with("typeServer/")
    }

    /// TSP-aware main loop that handles both TSP and LSP messages.
    pub(crate) fn tsp_main_loop(&mut self) -> crate::Result<()> {
        self.initialize(&Client::new(
            self.main_loop_sender.clone(),
            self.connection.sender.clone(),
        ));

        let mut scheduler = Scheduler::new(self.worker_threads);

        while let Ok(next_event) = self.next_event() {
            let Some(next_event) = next_event else {
                anyhow::bail!("client exited without proper shutdown sequence");
            };

            let client = Client::new(
                self.main_loop_sender.clone(),
                self.connection.sender.clone(),
            );

            match next_event {
                Event::Message(msg) => {
                    let Some(msg) = self.session.should_defer_message(msg) else {
                        continue;
                    };

                    let task = match msg {
                        Message::Request(req) => {
                            self.session
                                .request_queue_mut()
                                .incoming_mut()
                                .register(req.id.clone(), req.method.clone());

                            if self.session.is_shutdown_requested() {
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
                                tsp_api::request(req)
                            } else {
                                api::request(req)
                            }
                        }
                        Message::Notification(notification) => {
                            if notification.method == lsp_types::notification::Exit::METHOD {
                                if !self.session.is_shutdown_requested() {
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

                    scheduler.dispatch(task, &mut self.session, client);
                }
                Event::Action(action) => match action {
                    Action::SendResponse(response) => {
                        // Filter out responses for already canceled requests.
                        if let Some((start_time, method)) = self
                            .session
                            .request_queue_mut()
                            .incoming_mut()
                            .complete(&response.id)
                        {
                            let duration = start_time.elapsed();
                            tracing::trace!(name: "message response", method, %response.id, duration = format_args!("{:0.2?}", duration));

                            self.connection.sender.send(Message::Response(response))?;
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
                            .session
                            .request_queue()
                            .incoming()
                            .is_pending(&request.id)
                        {
                            let task = if Self::is_tsp_request(&request.method) {
                                tsp_api::request(request)
                            } else {
                                api::request(request)
                            };
                            scheduler.dispatch(task, &mut self.session, client);
                        } else {
                            tracing::debug!(
                                "Request {}/{} was cancelled, not retrying",
                                request.method,
                                request.id
                            );
                        }
                    }

                    Action::SendRequest(request) => client.send_request_raw(&self.session, request),

                    Action::SuspendWorkspaceDiagnostics(suspended_request) => {
                        self.session.set_suspended_workspace_diagnostics_request(
                            *suspended_request,
                            &client,
                        );
                    }

                    Action::InitializeWorkspaces(workspaces_with_options) => {
                        self.session
                            .initialize_workspaces(workspaces_with_options, &client);
                        // We do this here after workspaces have been initialized
                        // so that the file watcher globs can take project search
                        // paths into account.
                        // self.try_register_file_watcher(&client);
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

    /// Processes a TSP request from the client to the server.
    pub(super) fn request(req: server::Request) -> Task {
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
                let request_id = match id {
                    serde_json::Value::String(s) => lsp_server::RequestId::from(s),
                    serde_json::Value::Number(n) => {
                        if let Some(i) = n.as_i64() {
                            #[allow(clippy::cast_possible_truncation)]
                            lsp_server::RequestId::from(i as i32)
                        } else {
                            let result: crate::server::Result<()> =
                                Err(crate::server::api::Error::new(
                                    anyhow!("Invalid request ID format"),
                                    server::ErrorCode::InvalidRequest,
                                ));
                            return Task::immediate(request_id, result);
                        }
                    }
                    _ => {
                        let result: crate::server::Result<()> =
                            Err(crate::server::api::Error::new(
                                anyhow!("Invalid request ID format"),
                                server::ErrorCode::InvalidRequest,
                            ));
                        return Task::immediate(request_id, result);
                    }
                };

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
