//! Integration tests for Type Server Protocol (TSP) functionality

use anyhow::Result;
use lsp_types::{Position, notification::PublishDiagnostics};
use ruff_db::system::SystemPath;

use crate::TestServerBuilder;

/// Test typeServer/getType request for a simple variable
#[test]
fn get_type_simple_variable() -> Result<()> {
    let workspace_root = SystemPath::new("src");
    let foo_py = SystemPath::new("src/foo.py");
    let foo_content = "\
x = 42
y = \"hello\"
z = [1, 2, 3]
";

    let mut server = TestServerBuilder::new()?
        .with_tsp()
        .with_workspace(workspace_root, None)?
        .with_file(foo_py, foo_content)?
        .build()?
        .wait_until_workspaces_are_initialized()?;

    server.open_text_document(foo_py, &foo_content, 1);

    // Wait for publish diagnostics notification
    let _ = server.await_notification::<PublishDiagnostics>()?;

    // Test getting type of variable 'x' at position (0, 0)
    let type_result = server.tsp_get_type_request(foo_py, Position::new(0, 0))?;

    // For now, we expect a basic type response since implementation is placeholder
    // This test validates the TSP infrastructure works
    assert!(type_result.name.len() > 0, "Type name should not be empty");

    insta::assert_json_snapshot!("get_type_simple_variable", type_result);

    Ok(())
}

/// Test typeServer/getType request for a function definition
#[test]
fn get_type_function() -> Result<()> {
    let workspace_root = SystemPath::new("src");
    let foo_py = SystemPath::new("src/foo.py");
    let foo_content = "\
def add(a: int, b: int) -> int:
    return a + b

result = add(1, 2)
";

    let mut server = TestServerBuilder::new()?
        .with_tsp()
        .with_workspace(workspace_root, None)?
        .with_file(foo_py, foo_content)?
        .build()?
        .wait_until_workspaces_are_initialized()?;

    server.open_text_document(foo_py, &foo_content, 1);

    // Wait for publish diagnostics notification
    let _ = server.await_notification::<PublishDiagnostics>()?;

    // Test getting type of function 'add' at position (0, 4)
    let type_result = server.tsp_get_type_request(foo_py, Position::new(0, 4))?;

    assert!(type_result.name.len() > 0, "Type name should not be empty");

    insta::assert_json_snapshot!("get_type_function", type_result);

    Ok(())
}

/// Test typeServer/getType request for a class method
#[test]
fn get_type_class_method() -> Result<()> {
    let workspace_root = SystemPath::new("src");
    let foo_py = SystemPath::new("src/foo.py");
    let foo_content = "\
class Calculator:
    def __init__(self, value: int = 0):
        self.value = value
    
    def add(self, other: int) -> int:
        return self.value + other

calc = Calculator(10)
result = calc.add(5)
";

    let mut server = TestServerBuilder::new()?
        .with_tsp()
        .with_workspace(workspace_root, None)?
        .with_file(foo_py, foo_content)?
        .build()?
        .wait_until_workspaces_are_initialized()?;

    server.open_text_document(foo_py, &foo_content, 1);

    // Wait for publish diagnostics notification
    let _ = server.await_notification::<PublishDiagnostics>()?;

    // Test getting type of method 'add' at position (4, 8)
    let type_result = server.tsp_get_type_request(foo_py, Position::new(4, 8))?;

    assert!(type_result.name.len() > 0, "Type name should not be empty");

    insta::assert_json_snapshot!("get_type_class_method", type_result);

    Ok(())
}

/// Test typeServer/getType request with invalid position
#[test]
fn get_type_invalid_position() -> Result<()> {
    let workspace_root = SystemPath::new("src");
    let foo_py = SystemPath::new("src/foo.py");
    let foo_content = "\
x = 42
";

    let mut server = TestServerBuilder::new()?
        .with_tsp()
        .with_workspace(workspace_root, None)?
        .with_file(foo_py, foo_content)?
        .build()?
        .wait_until_workspaces_are_initialized()?;

    server.open_text_document(foo_py, &foo_content, 1);

    // Wait for publish diagnostics notification
    let _ = server.await_notification::<PublishDiagnostics>()?;

    // Test getting type at invalid position (10, 10) - beyond file content
    let type_result = server.tsp_get_type_request(foo_py, Position::new(10, 10))?;

    // Should still return a response (even if it's "Unknown")
    assert!(type_result.name.len() > 0, "Type name should not be empty");

    insta::assert_json_snapshot!("get_type_invalid_position", type_result);

    Ok(())
}

/// Test typeServer/getType request for complex expressions
#[test]
fn get_type_complex_expression() -> Result<()> {
    let workspace_root = SystemPath::new("src");
    let foo_py = SystemPath::new("src/foo.py");
    let foo_content = "\
from typing import List, Dict, Optional

data: List[Dict[str, Optional[int]]] = [
    {\"a\": 1, \"b\": None},
    {\"c\": 3, \"d\": 4}
]

first_item = data[0]
value = first_item.get(\"a\")
";

    let mut server = TestServerBuilder::new()?
        .with_tsp()
        .with_workspace(workspace_root, None)?
        .with_file(foo_py, foo_content)?
        .build()?
        .wait_until_workspaces_are_initialized()?;

    server.open_text_document(foo_py, &foo_content, 1);

    // Wait for publish diagnostics notification - there may be multiple
    let _ = server.await_notification::<PublishDiagnostics>()?;

    // Test getting type of 'data' variable at position (2, 0)
    let type_result = server.tsp_get_type_request(foo_py, Position::new(2, 0))?;

    assert!(type_result.name.len() > 0, "Type name should not be empty");

    insta::assert_json_snapshot!("get_type_complex_expression", type_result);

    Ok(())
}

/// Test typeServer/getSupportedProtocolVersion request
#[test]
fn get_supported_protocol_version() -> Result<()> {
    let workspace_root = SystemPath::new("src");

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
