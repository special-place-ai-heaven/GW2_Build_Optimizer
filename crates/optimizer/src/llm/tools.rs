//! Provider-neutral tool definitions.
//! Converts the Gemini-specific `FunctionDeclaration` format to `ToolDefinition`.
//! The `execute_tool()` function in `gemini_tools.rs` remains unchanged —
//! it's already provider-agnostic (takes `&str` name + `&Value` args).

use super::ToolDefinition;
use crate::gemini_tools;

/// Build all tool definitions in provider-neutral format.
/// Each provider's `LlmClient` implementation converts these to its wire format.
pub fn tool_definitions() -> Vec<ToolDefinition> {
    // Reuse the existing Gemini declarations and convert
    let gemini_tools = gemini_tools::tool_declarations();
    gemini_tools
        .into_iter()
        .flat_map(|tool| tool.function_declarations)
        .map(|decl| ToolDefinition {
            name: decl.name,
            description: decl.description,
            parameters: decl.parameters,
        })
        .collect()
}
