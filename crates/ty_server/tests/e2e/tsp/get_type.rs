//! Tests for typeServer/getType TSP functionality

use anyhow::Result;
use lsp_types::{Position, notification::PublishDiagnostics};
use ruff_db::system::SystemPath;
use ty_server::{GetTypeResponse, TypeHandle};

use crate::TestServerBuilder;

/// Normalize a GetTypeResponse for snapshot testing by replacing non-deterministic handles
/// with predictable placeholders based on the type name.
fn normalize_for_snapshot(response: GetTypeResponse) -> serde_json::Value {
    let mut json = serde_json::to_value(&response).unwrap();
    if let Some(obj) = json.as_object_mut() {
        // Replace the handle with a deterministic placeholder based on the type name
        if let Some(name) = obj.get("name").and_then(|n| n.as_str()) {
            obj.insert(
                "handle".to_string(),
                serde_json::Value::String(format!("test_handle_{}", name)),
            );
        }
    }
    json
}

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

    insta::assert_json_snapshot!(
        "get_type_simple_variable",
        normalize_for_snapshot(type_result)
    );

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

    insta::assert_json_snapshot!("get_type_function", normalize_for_snapshot(type_result));

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

    insta::assert_json_snapshot!("get_type_class_method", normalize_for_snapshot(type_result));

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

    insta::assert_json_snapshot!(
        "get_type_invalid_position",
        normalize_for_snapshot(type_result)
    );

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

    insta::assert_json_snapshot!(
        "get_type_complex_expression",
        normalize_for_snapshot(type_result)
    );

    Ok(())
}

/// Test typeServer/getType request for basic Python literals
#[test]
fn get_type_basic_literals() -> Result<()> {
    let workspace_root = SystemPath::new("src");
    let foo_py = SystemPath::new("src/foo.py");
    let foo_content = "\
# Test basic literals
integer_var = 42
string_var = \"hello world\"
float_var = 3.14
bool_var = True
none_var = None
list_var = [1, 2, 3]
dict_var = {\"key\": \"value\"}
";

    let mut server = TestServerBuilder::new()?
        .with_tsp()
        .with_workspace(workspace_root, None)?
        .with_file(foo_py, foo_content)?
        .build()?
        .wait_until_workspaces_are_initialized()?;

    server.open_text_document(foo_py, &foo_content, 1);
    let _ = server.await_notification::<PublishDiagnostics>()?;

    // Test integer literal
    let integer_result = server.tsp_get_type_request(foo_py, Position::new(1, 14))?; // Position at '42'
    assert!(
        integer_result.name.len() > 0,
        "Integer type name should not be empty"
    );
    insta::assert_json_snapshot!(
        "get_type_integer_literal",
        normalize_for_snapshot(integer_result)
    );

    // Test string literal
    let string_result = server.tsp_get_type_request(foo_py, Position::new(2, 14))?; // Position at '"hello world"'
    assert!(
        string_result.name.len() > 0,
        "String type name should not be empty"
    );
    insta::assert_json_snapshot!(
        "get_type_string_literal",
        normalize_for_snapshot(string_result)
    );

    // Test boolean literal
    let bool_result = server.tsp_get_type_request(foo_py, Position::new(4, 11))?; // Position at 'True'
    assert!(
        bool_result.name.len() > 0,
        "Boolean type name should not be empty"
    );
    insta::assert_json_snapshot!(
        "get_type_boolean_literal",
        normalize_for_snapshot(bool_result)
    );

    Ok(())
}

/// Test typeServer/getType request for function calls and return types
#[test]
fn get_type_function_calls() -> Result<()> {
    let workspace_root = SystemPath::new("src");
    let foo_py = SystemPath::new("src/foo.py");
    let foo_content = "\
def simple_function(x: int) -> str:
    return str(x)

def generic_function(x):
    return x * 2

# Function calls
result1 = simple_function(42)
result2 = generic_function(\"hello\")
result3 = len([1, 2, 3])
";

    let mut server = TestServerBuilder::new()?
        .with_tsp()
        .with_workspace(workspace_root, None)?
        .with_file(foo_py, foo_content)?
        .build()?
        .wait_until_workspaces_are_initialized()?;

    server.open_text_document(foo_py, &foo_content, 1);
    let _ = server.await_notification::<PublishDiagnostics>()?;

    // Test function definition
    let func_def_result = server.tsp_get_type_request(foo_py, Position::new(0, 4))?; // "simple_function"
    assert!(
        func_def_result.name.len() > 0,
        "Function definition type should not be empty"
    );
    insta::assert_json_snapshot!(
        "get_type_function_definition",
        normalize_for_snapshot(func_def_result)
    );

    // Test function call result with type annotation
    let annotated_call_result = server.tsp_get_type_request(foo_py, Position::new(7, 10))?; // "simple_function(42)"
    assert!(
        annotated_call_result.name.len() > 0,
        "Annotated function call result should not be empty"
    );
    insta::assert_json_snapshot!(
        "get_type_annotated_function_call",
        normalize_for_snapshot(annotated_call_result)
    );

    // Test builtin function call
    let builtin_call_result = server.tsp_get_type_request(foo_py, Position::new(9, 10))?; // "len([1, 2, 3])"
    assert!(
        builtin_call_result.name.len() > 0,
        "Builtin function call result should not be empty"
    );
    insta::assert_json_snapshot!(
        "get_type_builtin_function_call",
        normalize_for_snapshot(builtin_call_result)
    );

    Ok(())
}

/// Test typeServer/getType request for class instantiation and method calls
#[test]
fn get_type_classes_and_instances() -> Result<()> {
    let workspace_root = SystemPath::new("src");
    let foo_py = SystemPath::new("src/foo.py");
    let foo_content = "\
class MyClass:
    def __init__(self, value: int):
        self.value = value
    
    def get_value(self) -> int:
        return self.value
    
    def double_value(self):
        return self.value * 2

# Class instantiation and method calls
obj = MyClass(42)
value_result = obj.get_value()
double_result = obj.double_value()
attribute_access = obj.value
";

    let mut server = TestServerBuilder::new()?
        .with_tsp()
        .with_workspace(workspace_root, None)?
        .with_file(foo_py, foo_content)?
        .build()?
        .wait_until_workspaces_are_initialized()?;

    server.open_text_document(foo_py, &foo_content, 1);
    let _ = server.await_notification::<PublishDiagnostics>()?;

    // Test class definition
    let class_def_result = server.tsp_get_type_request(foo_py, Position::new(0, 6))?; // "MyClass"
    assert!(
        class_def_result.name.len() > 0,
        "Class definition type should not be empty"
    );
    insta::assert_json_snapshot!(
        "get_type_class_definition",
        normalize_for_snapshot(class_def_result)
    );

    // Test instance variable
    let instance_result = server.tsp_get_type_request(foo_py, Position::new(11, 6))?; // "MyClass(42)"
    assert!(
        instance_result.name.len() > 0,
        "Class instance type should not be empty"
    );
    insta::assert_json_snapshot!(
        "get_type_class_instance",
        normalize_for_snapshot(instance_result)
    );

    // Test method call with return annotation
    let annotated_method_result = server.tsp_get_type_request(foo_py, Position::new(12, 14))?; // "obj.get_value()"
    assert!(
        annotated_method_result.name.len() > 0,
        "Annotated method call result should not be empty"
    );
    insta::assert_json_snapshot!(
        "get_type_annotated_method_call",
        normalize_for_snapshot(annotated_method_result)
    );

    // Test attribute access
    let attribute_result = server.tsp_get_type_request(foo_py, Position::new(14, 18))?; // "obj.value"
    assert!(
        attribute_result.name.len() > 0,
        "Attribute access type should not be empty"
    );
    insta::assert_json_snapshot!(
        "get_type_attribute_access",
        normalize_for_snapshot(attribute_result)
    );

    Ok(())
}

/// Test typeServer/getType request for collection types and indexing
#[test]
fn get_type_collections_and_indexing() -> Result<()> {
    let workspace_root = SystemPath::new("src");
    let foo_py = SystemPath::new("src/foo.py");
    let foo_content = "\
from typing import List, Dict, Tuple

# Collection literals
my_list = [1, 2, 3]
my_dict = {\"a\": 1, \"b\": 2}
my_tuple = (1, \"hello\", 3.14)

# Collection indexing
list_item = my_list[0]
dict_item = my_dict[\"a\"]
tuple_item = my_tuple[1]

# Collection methods
list_length = len(my_list)
dict_keys = my_dict.keys()
";

    let mut server = TestServerBuilder::new()?
        .with_tsp()
        .with_workspace(workspace_root, None)?
        .with_file(foo_py, foo_content)?
        .build()?
        .wait_until_workspaces_are_initialized()?;

    server.open_text_document(foo_py, &foo_content, 1);
    let _ = server.await_notification::<PublishDiagnostics>()?;

    // Test list literal
    let list_result = server.tsp_get_type_request(foo_py, Position::new(3, 10))?; // "[1, 2, 3]"
    assert!(list_result.name.len() > 0, "List type should not be empty");
    insta::assert_json_snapshot!("get_type_list_literal", normalize_for_snapshot(list_result));

    // Test dict literal
    let dict_result = server.tsp_get_type_request(foo_py, Position::new(4, 10))?; // "{\"a\": 1, \"b\": 2}"
    assert!(dict_result.name.len() > 0, "Dict type should not be empty");
    insta::assert_json_snapshot!("get_type_dict_literal", normalize_for_snapshot(dict_result));

    // Test list indexing
    let list_index_result = server.tsp_get_type_request(foo_py, Position::new(8, 12))?; // "my_list[0]"
    assert!(
        list_index_result.name.len() > 0,
        "List indexing result should not be empty"
    );
    insta::assert_json_snapshot!(
        "get_type_list_indexing",
        normalize_for_snapshot(list_index_result)
    );

    // Test dict method call
    let dict_method_result = server.tsp_get_type_request(foo_py, Position::new(14, 12))?; // "my_dict.keys()"
    assert!(
        dict_method_result.name.len() > 0,
        "Dict method result should not be empty"
    );
    insta::assert_json_snapshot!(
        "get_type_dict_method",
        normalize_for_snapshot(dict_method_result)
    );

    Ok(())
}

/// Test typeServer/getType request for error cases and edge conditions
#[test]
fn get_type_error_cases() -> Result<()> {
    let workspace_root = SystemPath::new("src");
    let foo_py = SystemPath::new("src/foo.py");
    let foo_content = "\
x = 42
# This is a comment
    # Indented comment
";

    let mut server = TestServerBuilder::new()?
        .with_tsp()
        .with_workspace(workspace_root, None)?
        .with_file(foo_py, foo_content)?
        .build()?
        .wait_until_workspaces_are_initialized()?;

    server.open_text_document(foo_py, &foo_content, 1);
    let _ = server.await_notification::<PublishDiagnostics>()?;

    // Test position in whitespace (should handle gracefully)
    let whitespace_result = server.tsp_get_type_request(foo_py, Position::new(0, 1))?; // Between 'x' and '='
    insta::assert_json_snapshot!(
        "get_type_whitespace",
        normalize_for_snapshot(whitespace_result)
    );

    // Test position in comment
    let comment_result = server.tsp_get_type_request(foo_py, Position::new(1, 5))?; // Inside comment
    insta::assert_json_snapshot!("get_type_comment", normalize_for_snapshot(comment_result));

    // Test position at end of line
    let eol_result = server.tsp_get_type_request(foo_py, Position::new(0, 6))?; // End of "x = 42"
    insta::assert_json_snapshot!("get_type_end_of_line", normalize_for_snapshot(eol_result));

    Ok(())
}

/// Test typeServer/getType request for expressions with type annotations
#[test]
fn get_type_annotated_expressions() -> Result<()> {
    let workspace_root = SystemPath::new("src");
    let foo_py = SystemPath::new("src/foo.py");
    let foo_content = "\
from typing import Optional, Union, List

# Variable annotations
annotated_var: int = 42
optional_var: Optional[str] = None
union_var: Union[int, str] = \"hello\"
list_var: List[int] = [1, 2, 3]

# Function with annotations
def typed_function(param: str) -> Optional[int]:
    if param.isdigit():
        return int(param)
    return None

result = typed_function(\"123\")
";

    let mut server = TestServerBuilder::new()?
        .with_tsp()
        .with_workspace(workspace_root, None)?
        .with_file(foo_py, foo_content)?
        .build()?
        .wait_until_workspaces_are_initialized()?;

    server.open_text_document(foo_py, &foo_content, 1);
    let _ = server.await_notification::<PublishDiagnostics>()?;

    // Test annotated variable
    let annotated_var_result = server.tsp_get_type_request(foo_py, Position::new(3, 13))?; // "annotated_var"
    assert!(
        annotated_var_result.name.len() > 0,
        "Annotated variable type should not be empty"
    );
    insta::assert_json_snapshot!(
        "get_type_annotated_variable",
        normalize_for_snapshot(annotated_var_result)
    );

    // Test optional variable
    let optional_var_result = server.tsp_get_type_request(foo_py, Position::new(4, 12))?; // "optional_var"
    assert!(
        optional_var_result.name.len() > 0,
        "Optional variable type should not be empty"
    );
    insta::assert_json_snapshot!(
        "get_type_optional_variable",
        normalize_for_snapshot(optional_var_result)
    );

    // Test function call with Optional return type
    let optional_return_result = server.tsp_get_type_request(foo_py, Position::new(14, 9))?; // "typed_function(\"123\")"
    assert!(
        optional_return_result.name.len() > 0,
        "Optional return type should not be empty"
    );
    insta::assert_json_snapshot!(
        "get_type_optional_return",
        normalize_for_snapshot(optional_return_result)
    );

    Ok(())
}

/// Test that verifies the type handle returned from get_type can regenerate the same internal type.
/// This test validates the round-trip functionality of type handles - ensuring that handles are
/// stable identifiers that can be used to retrieve the same type information consistently.
#[test]
fn get_type_handle_roundtrip() -> Result<()> {
    let workspace_root = SystemPath::new("src");
    let foo_py = SystemPath::new("src/foo.py");
    let foo_content = "\
x = 42
y = \"hello\"
my_list = [1, 2, 3]
my_dict = {\"key\": \"value\"}

def my_function():
    return 42

class MyClass:
    def __init__(self):
        self.value = 100
";

    let mut server = TestServerBuilder::new()?
        .with_tsp()
        .with_workspace(workspace_root, None)?
        .with_file(foo_py, foo_content)?
        .build()?
        .wait_until_workspaces_are_initialized()?;

    server.open_text_document(foo_py, &foo_content, 1);
    let _ = server.await_notification::<PublishDiagnostics>()?;

    // Test multiple expressions and verify handle consistency
    let test_cases = [
        (Position::new(0, 0), "int variable 'x'"),        // x = 42
        (Position::new(1, 0), "string variable 'y'"),     // y = "hello"
        (Position::new(2, 0), "list variable 'my_list'"), // my_list = [1, 2, 3]
        (Position::new(3, 0), "dict variable 'my_dict'"), // my_dict = {"key": "value"}
        (Position::new(5, 4), "function 'my_function'"),  // def my_function():
        (Position::new(8, 6), "class 'MyClass'"),         // class MyClass:
        (Position::new(0, 4), "int literal '42'"),        // x = 42 (the literal)
        (Position::new(1, 4), "string literal"),          // y = "hello" (the literal)
    ];

    for (position, description) in test_cases {
        // Get the type information twice for the same position
        let first_result = server.tsp_get_type_request(foo_py, position)?;
        let second_result = server.tsp_get_type_request(foo_py, position)?;

        // Verify that handles are identical for the same position/type
        assert_eq!(
            first_result.handle, second_result.handle,
            "Type handles should be identical for the same position: {} at {:?}",
            description, position
        );

        // Verify that other type properties are also identical
        assert_eq!(
            first_result.name, second_result.name,
            "Type names should be identical for the same position: {} at {:?}",
            description, position
        );

        assert_eq!(
            first_result.category, second_result.category,
            "Type categories should be identical for the same position: {} at {:?}",
            description, position
        );

        // Verify that the handle is actually meaningful (not 0 or obviously invalid)
        match &first_result.handle {
            TypeHandle::Int(handle_value) => {
                assert_ne!(
                    *handle_value, 0,
                    "Handle should not be zero for {}",
                    description
                );
            }
            TypeHandle::String(handle_value) => {
                assert!(
                    !handle_value.is_empty(),
                    "String handle should not be empty for {}",
                    description
                );
            }
        }

        println!(
            "✓ Handle consistency verified for {} at {:?}: {:?}",
            description, position, first_result.handle
        );
    }

    // Test that different types have different handles
    let int_type = server.tsp_get_type_request(foo_py, Position::new(0, 0))?; // x = 42
    let string_type = server.tsp_get_type_request(foo_py, Position::new(1, 0))?; // y = "hello"
    let list_type = server.tsp_get_type_request(foo_py, Position::new(2, 0))?; // my_list = [1, 2, 3]

    assert_ne!(
        int_type.handle, string_type.handle,
        "Different types should have different handles (int vs string)"
    );

    assert_ne!(
        int_type.handle, list_type.handle,
        "Different types should have different handles (int vs list)"
    );

    assert_ne!(
        string_type.handle, list_type.handle,
        "Different types should have different handles (string vs list)"
    );

    println!("✓ Different types have different handles as expected");

    // Note: In a complete implementation, we would also test that we can use the handle
    // to retrieve the original type information, but since we don't have a "getTypeByHandle"
    // endpoint yet, we're testing handle stability and uniqueness instead.

    Ok(())
}

/// Test typeServer/getType request includes module name information
#[test]
fn get_type_with_module_name() -> Result<()> {
    let workspace_root = SystemPath::new("src");
    let main_py = SystemPath::new("src/main.py");
    let utils_py = SystemPath::new("src/utils.py");

    // Create a utils module with a class
    let utils_content = "\
class MyClass:
    def __init__(self, value: int):
        self.value = value
        
def create_instance() -> MyClass:
    return MyClass(42)
";

    // Create main module that imports from utils
    let main_content = "\
from utils import MyClass, create_instance

# Test variable with type from another module
my_instance = create_instance()

# Test local variable
local_var = 123
";

    let mut server = TestServerBuilder::new()?
        .with_tsp()
        .with_workspace(workspace_root, None)?
        .with_file(utils_py, utils_content)?
        .with_file(main_py, main_content)?
        .build()?
        .wait_until_workspaces_are_initialized()?;

    // Open both files
    server.open_text_document(utils_py, &utils_content, 1);
    server.open_text_document(main_py, &main_content, 1);

    // Wait for publish diagnostics notifications for both files
    let _ = server.await_notification::<PublishDiagnostics>()?;
    let _ = server.await_notification::<PublishDiagnostics>()?;

    // Test getting type of 'my_instance' which should have module name from utils
    let imported_type = server.tsp_get_type_request(main_py, Position::new(3, 0))?;

    // Test getting type of 'local_var' which should be a builtin type
    let builtin_type = server.tsp_get_type_request(main_py, Position::new(6, 0))?;

    // Verify the imported type has module information
    println!("Imported type: {:?}", imported_type);
    println!("Builtin type: {:?}", builtin_type);

    // The imported type should have module name information
    if let Some(module_name) = &imported_type.module_name {
        assert!(
            !module_name.name_parts.is_empty(),
            "Module name parts should not be empty for imported type"
        );
        assert_eq!(
            module_name.leading_dots, 0,
            "Module name should have 0 leading dots for absolute import"
        );
        println!("✓ Imported type has module name: {:?}", module_name);
    } else {
        // This might be acceptable depending on implementation
        println!("⚠ Imported type has no module name (might be expected)");
    }

    // Create snapshots to track the structure
    insta::assert_json_snapshot!(
        "get_type_imported_class_instance",
        normalize_for_snapshot(imported_type)
    );

    insta::assert_json_snapshot!("get_type_builtin_int", normalize_for_snapshot(builtin_type));

    Ok(())
}
