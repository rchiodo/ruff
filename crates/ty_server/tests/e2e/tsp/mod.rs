//! Integration tests for Type Server Protocol (TSP) functionality

use anyhow::Result;

use crate::TestServerBuilder;

pub mod get_type;

/// Test typeServer/getSupportedProtocolVersion request
#[test]
fn get_supported_protocol_version() -> Result<()> {
    let workspace_root = ruff_db::system::SystemPath::new("src");

    let mut server = TestServerBuilder::new()?
        .with_tsp()
        .with_workspace(workspace_root, None)?
        .build()?
        .wait_until_workspaces_are_initialized()?;

    // Test getting the supported protocol version
    let version = server.tsp_get_supported_protocol_version_request()?;

    // Should return the version from the protocol.rs file
    assert_eq!(version, "0.2.0");

    // Verify it's a valid semver format
    assert!(
        version.chars().filter(|&c| c == '.').count() == 2,
        "Version should have 2 dots for semver format"
    );
    assert!(
        version.split('.').all(|part| part.parse::<u32>().is_ok()),
        "All parts should be numbers"
    );

    Ok(())
}
