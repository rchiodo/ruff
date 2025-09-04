use lsp_types::request::Request;
use ty_project::ProjectDatabase;

use crate::server::tsp::protocol::{GetTypeArgsParams, GetTypeArgsResponse};
use crate::session::DocumentSnapshot;
use crate::session::client::Client;

use super::common::TspCommon;

// Define the TSP GetTypeArgs request
#[allow(dead_code)]
pub(crate) struct GetTypeArgsRequest;

impl Request for GetTypeArgsRequest {
    type Params = GetTypeArgsParams;
    type Result = GetTypeArgsResponse;
    const METHOD: &'static str = "typeServer/getTypeArgs";
}

pub(crate) struct GetTypeArgsRequestHandler;

impl GetTypeArgsRequestHandler {
    pub(crate) fn handle_request(
        id: &lsp_server::RequestId,
        db: &ProjectDatabase,
        snapshot: &DocumentSnapshot,
        client: &Client,
        params: &GetTypeArgsParams,
    ) {
        let result = Self::run_request(db, snapshot, params);

        if let Err(err) = &result {
            tracing::error!("An error occurred with request ID {id}: {err}");
            client.show_error_message(
                "ty encountered a problem with getTypeArgs. Check the logs for more details.",
            );
        }

        client.respond(id, result);
    }

    fn run_request(
        db: &ProjectDatabase,
        _snapshot: &DocumentSnapshot,
        params: &GetTypeArgsParams,
    ) -> crate::server::Result<GetTypeArgsResponse> {
        // According to the TSP protocol, GetTypeArgsParams only has `snapshot` and `type` fields
        // We need to resolve the type from the handle and extract its arguments

        let type_handle = &params.type_.handle;
        let type_name = &params.type_.name;

        // Try to resolve the type from the handle (placeholder implementation)
        match TspCommon::resolve_type_from_handle(type_handle.clone(), type_name) {
            Ok(semantic_type) => {
                // Extract type arguments from the semantic type
                let type_args = TspCommon::extract_type_args(db, &semantic_type);
                Ok(type_args)
            }
            Err(_) => {
                // For now, return an empty list if we can't resolve the type
                // In a real implementation, we might want to return an error
                tracing::warn!(
                    "Could not resolve type from handle {:?} with name '{}'",
                    type_handle,
                    type_name
                );
                Ok(vec![])
            }
        }
    }
}
