use std::num::NonZeroUsize;
use std::panic::RefUnwindSafe;
use std::sync::Arc;

use lsp_server::{Connection, Message};
use ruff_db::system::System;

pub struct Server {
    inner: ty_server::Server,
    incoming_forwarder: Option<std::thread::JoinHandle<()>>,
}

impl Server {
    pub fn new(
        worker_threads: NonZeroUsize,
        connection: Connection,
        native_system: Arc<dyn System + 'static + Send + Sync + RefUnwindSafe>,
        in_test: bool,
    ) -> anyhow::Result<Self> {
        let (connection, incoming_forwarder) = Self::wrap_connection(connection);

        let inner = ty_server::Server::new(worker_threads, connection, native_system, in_test)?;

        Ok(Self {
            inner,
            incoming_forwarder: Some(incoming_forwarder),
        })
    }

    pub fn run(mut self) -> anyhow::Result<()> {
        let result = self.inner.run();

        if let Some(handle) = self.incoming_forwarder.take() {
            let _ = handle.join();
        }

        result
    }

    fn wrap_connection(connection: Connection) -> (Connection, std::thread::JoinHandle<()>) {
        let Connection { sender, receiver } = connection;

        let (forward_sender, forward_receiver) = crossbeam::channel::unbounded::<Message>();

        let handle = std::thread::spawn(move || {
            while let Ok(message) = receiver.recv() {
                if Self::try_handle_custom_message(&message) {
                    continue;
                }

                if forward_sender.send(message).is_err() {
                    break;
                }
            }
        });

        (
            Connection {
                sender,
                receiver: forward_receiver,
            },
            handle,
        )
    }

    fn try_handle_custom_message(_message: &Message) -> bool {
        false
    }
}
