use std::num::NonZeroUsize;
use std::sync::Arc;
use std::time::{Duration, Instant};

use lsp_server::{Connection, Message, Request, RequestId, Response};
use lsp_types::request::{Initialize, Request as _};
use lsp_types::{ClientCapabilities, InitializeParams, InitializeResult, Url, WorkspaceFolder};
use ruff_db::system::{OsSystem, SystemPathBuf, TestSystem};

fn send_request(connection: &Connection, request: Request) {
    connection
        .sender
        .send(Message::Request(request))
        .expect("send request");
}

fn send_response(connection: &Connection, response: Response) {
    connection
        .sender
        .send(Message::Response(response))
        .expect("send response");
}

fn send_notification(connection: &Connection, method: &str, params: serde_json::Value) {
    connection
        .sender
        .send(Message::Notification(lsp_server::Notification {
            method: method.to_string(),
            params,
        }))
        .expect("send notification");
}

fn recv_until_response(
    connection: &Connection,
    wanted_id: &RequestId,
    deadline: Instant,
) -> Response {
    loop {
        let now = Instant::now();
        assert!(
            now < deadline,
            "timed out waiting for response {wanted_id:?}"
        );

        let timeout = deadline.saturating_duration_since(now);
        match connection.receiver.recv_timeout(timeout) {
            Ok(Message::Response(response)) if &response.id == wanted_id => return response,
            Ok(Message::Request(request)) => {
                // Respond to common server->client requests to avoid deadlocks.
                if request.method == "workspace/configuration" {
                    let params: lsp_types::ConfigurationParams =
                        serde_json::from_value(request.params).expect("parse configuration params");
                    let values = vec![serde_json::Value::Null; params.items.len()];
                    send_response(
                        connection,
                        Response {
                            id: request.id,
                            result: Some(serde_json::Value::Array(values)),
                            error: None,
                        },
                    );
                } else {
                    // Default: respond with null result.
                    send_response(
                        connection,
                        Response {
                            id: request.id,
                            result: Some(serde_json::Value::Null),
                            error: None,
                        },
                    );
                }
            }
            Ok(Message::Notification(_)) => {
                // Ignore notifications.
            }
            Err(err) => panic!("recv failed: {err}"),
            _ => {}
        }
    }
}

#[test]
fn tsp_smoke_initialize_shutdown_exit() {
    let (server_connection, client_connection) = Connection::memory();

    let temp_dir = tempfile::tempdir().expect("create temp dir");

    let cwd = SystemPathBuf::from_path_buf(temp_dir.path().to_path_buf())
        .expect("temp dir path is valid unicode");
    let os_system = OsSystem::new(cwd);
    let system = Arc::new(TestSystem::new(os_system));

    let server_thread = std::thread::spawn({
        let system =
            system as Arc<dyn ruff_db::system::System + Send + Sync + std::panic::RefUnwindSafe>;
        move || {
            let worker_threads = NonZeroUsize::new(1).unwrap();
            let server = tsp_server::Server::new(worker_threads, server_connection, system, true)
                .expect("create tsp server");
            server.run().expect("server run");
        }
    });

    let root_uri = Url::from_file_path(temp_dir.path()).unwrap();

    let init_params = InitializeParams {
        capabilities: ClientCapabilities::default(),
        workspace_folders: Some(vec![WorkspaceFolder {
            uri: root_uri,
            name: "workspace".to_string(),
        }]),
        ..Default::default()
    };

    let init_request = Request {
        id: RequestId::from(1),
        method: Initialize::METHOD.to_string(),
        params: serde_json::to_value(init_params).unwrap(),
    };

    send_request(&client_connection, init_request);

    let init_request_id = RequestId::from(1);
    let deadline = Instant::now() + Duration::from_secs(10);
    let init_response = recv_until_response(&client_connection, &init_request_id, deadline);

    let init_result: InitializeResult = serde_json::from_value(
        init_response
            .result
            .expect("initialize response has result"),
    )
    .expect("parse InitializeResult");

    let server_info = init_result.server_info.expect("serverInfo present");
    assert_eq!(server_info.name, "ty");

    send_notification(
        &client_connection,
        "initialized",
        serde_json::to_value(lsp_types::InitializedParams {}).unwrap(),
    );

    let shutdown_request = Request {
        id: RequestId::from(2),
        method: "shutdown".to_string(),
        params: serde_json::Value::Null,
    };
    send_request(&client_connection, shutdown_request);

    let shutdown_request_id = RequestId::from(2);
    let _shutdown_response = recv_until_response(
        &client_connection,
        &shutdown_request_id,
        Instant::now() + Duration::from_secs(10),
    );

    send_notification(&client_connection, "exit", serde_json::Value::Null);

    // Drop the client connection before joining the server thread to avoid hangs.
    drop(client_connection);

    // Ensure the server thread exits promptly.
    let (done_tx, done_rx) = crossbeam::channel::bounded(1);
    std::thread::spawn(move || {
        done_tx.send(server_thread.join()).ok();
    });

    let joined = done_rx
        .recv_timeout(Duration::from_secs(10))
        .expect("server thread should exit");
    joined.expect("server thread panicked");
}
