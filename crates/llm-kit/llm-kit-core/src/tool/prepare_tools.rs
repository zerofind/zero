use crate::tool::tool_set::ToolSet;
use llm_kit_provider::{
    language_model::tool::{
        LanguageModelTool, function_tool::LanguageModelFunctionTool,
        provider_defined_tool::LanguageModelProviderDefinedTool,
    },
    language_model::tool_choice::LanguageModelToolChoice,
};
use llm_kit_provider_utils::tool::Tool;

/// Prepares tools and tool choice for the language model.
///
/// Converts a ToolSet (HashMap of tool names to tools) into provider tools
/// and prepares the tool choice strategy.
///
/// # Arguments
///
/// * `tools` - Optional reference to tool set (HashMap of tool names to tools)
/// * `tool_choice` - Optional tool choice strategy
///
/// # Returns
///
/// A tuple of (`Option<Vec<LanguageModelTool>>`, `Option<ToolChoice>`)
///
/// # Example
///
/// ```no_run
/// use llm_kit_core::tool::{ToolSet, prepare_tools_and_tool_choice};
/// use llm_kit_provider_utils::tool::{Tool, ToolExecutionOutput};
/// use serde_json::json;
/// use std::sync::Arc;
///
/// let mut tools = ToolSet::new();
/// let tool = Tool::function(json!({"type": "object"}))
///     .with_execute(Arc::new(|_input, _opts| {
///         ToolExecutionOutput::Single(Box::pin(async move { Ok(json!({})) }))
///     }));
/// tools.insert("my_tool".to_string(), tool);
///
/// let (provider_tools, tool_choice) = prepare_tools_and_tool_choice(Some(&tools), None);
/// ```
pub fn prepare_tools_and_tool_choice(
    tools: Option<&ToolSet>,
    tool_choice: Option<LanguageModelToolChoice>,
) -> (
    Option<Vec<LanguageModelTool>>,
    Option<LanguageModelToolChoice>,
) {
    // If no tools provided, return None for both
    if tools.is_none() || tools.map(|t| t.is_empty()).unwrap_or(true) {
        return (None, None);
    }

    let tools = tools.unwrap();
    let mut language_model_tools = Vec::new();

    // Convert each tool in the toolset to a provider tool
    for (name, tool) in tools {
        let provider_tool = convert_tool_to_provider(name.clone(), tool);
        language_model_tools.push(provider_tool);
    }

    // Prepare tool choice - if not specified, default to auto
    let prepared_tool_choice = tool_choice.or(Some(LanguageModelToolChoice::Auto));

    (Some(language_model_tools), prepared_tool_choice)
}

/// Convert a core Tool to a provider Tool.
///
/// # Arguments
///
/// * `name` - The name of the tool (from the ToolSet key)
/// * `core_tool` - The tool definition
fn convert_tool_to_provider(name: String, core_tool: &Tool) -> LanguageModelTool {
    use llm_kit_provider_utils::tool::ToolType;

    match &core_tool.tool_type {
        ToolType::Function => {
            let mut function_tool =
                LanguageModelFunctionTool::new(name, core_tool.input_schema.clone());

            if let Some(desc) = &core_tool.description {
                function_tool = function_tool.with_description(desc.clone());
            }
            if let Some(opts) = &core_tool.provider_options {
                function_tool = function_tool.with_provider_options(opts.clone());
            }

            LanguageModelTool::Function(function_tool)
        }
        ToolType::Dynamic => {
            // Dynamic tools are treated as function tools in the provider
            let mut function_tool =
                LanguageModelFunctionTool::new(name, core_tool.input_schema.clone());

            if let Some(desc) = &core_tool.description {
                function_tool = function_tool.with_description(desc.clone());
            }
            if let Some(opts) = &core_tool.provider_options {
                function_tool = function_tool.with_provider_options(opts.clone());
            }

            LanguageModelTool::Function(function_tool)
        }
        ToolType::ProviderDefined { id, name: _, args } => {
            // For provider-defined tools, use the HashMap key as the name
            let provider_tool =
                LanguageModelProviderDefinedTool::new(id.clone(), name, args.clone());
            LanguageModelTool::ProviderDefined(provider_tool)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_prepare_none_tools_returns_none() {
        let (tools, choice) = prepare_tools_and_tool_choice(None, None);
        assert!(tools.is_none());
        assert!(choice.is_none());
    }

    #[test]
    fn test_prepare_empty_tools_returns_none() {
        let toolset = ToolSet::new();
        let (tools, choice) = prepare_tools_and_tool_choice(Some(&toolset), None);
        assert!(tools.is_none());
        assert!(choice.is_none());
    }

    #[test]
    fn test_prepare_function_tool() {
        let mut toolset = ToolSet::new();
        toolset.insert(
            "get_weather".to_string(),
            Tool::function(json!({
                "type": "object",
                "properties": {
                    "city": {"type": "string"}
                }
            }))
            .with_description("Get weather"),
        );

        let (tools, choice) = prepare_tools_and_tool_choice(Some(&toolset), None);

        assert!(tools.is_some());
        let tools = tools.unwrap();
        assert_eq!(tools.len(), 1);
        assert!(matches!(&tools[0], LanguageModelTool::Function(f) if f.name == "get_weather"));

        // Default tool choice is Auto
        assert!(matches!(choice, Some(LanguageModelToolChoice::Auto)));
    }

    #[test]
    fn test_prepare_dynamic_tool_as_function() {
        let mut toolset = ToolSet::new();
        toolset.insert(
            "dyn_tool".to_string(),
            Tool::dynamic(json!({"type": "object"})).with_description("Dynamic"),
        );

        let (tools, _) = prepare_tools_and_tool_choice(Some(&toolset), None);

        let tools = tools.unwrap();
        assert_eq!(tools.len(), 1);
        // Dynamic tools are converted as Function tools
        assert!(matches!(&tools[0], LanguageModelTool::Function(f) if f.name == "dyn_tool"));
    }

    #[test]
    fn test_prepare_provider_defined_tool() {
        let mut toolset = ToolSet::new();
        let mut args = std::collections::HashMap::new();
        args.insert("display_width".to_string(), json!(1920));
        toolset.insert(
            "computer".to_string(),
            Tool::provider_defined(
                "computer_20250124",
                "computer",
                args,
                json!({"type": "object"}),
            ),
        );

        let (tools, _) = prepare_tools_and_tool_choice(Some(&toolset), None);

        let tools = tools.unwrap();
        assert_eq!(tools.len(), 1);
        assert!(matches!(&tools[0], LanguageModelTool::ProviderDefined(_)));
    }

    #[test]
    fn test_prepare_custom_tool_choice() {
        let mut toolset = ToolSet::new();
        toolset.insert(
            "tool_a".to_string(),
            Tool::function(json!({"type": "object"})),
        );

        let (_, choice) =
            prepare_tools_and_tool_choice(Some(&toolset), Some(LanguageModelToolChoice::Required));

        assert!(matches!(choice, Some(LanguageModelToolChoice::Required)));
    }

    #[test]
    fn test_prepare_multiple_tools() {
        let mut toolset = ToolSet::new();
        toolset.insert(
            "tool_a".to_string(),
            Tool::function(json!({"type": "object"})),
        );
        toolset.insert(
            "tool_b".to_string(),
            Tool::function(json!({"type": "object"})),
        );
        toolset.insert(
            "tool_c".to_string(),
            Tool::dynamic(json!({"type": "object"})),
        );

        let (tools, _) = prepare_tools_and_tool_choice(Some(&toolset), None);

        let tools = tools.unwrap();
        assert_eq!(tools.len(), 3);
    }

    #[test]
    fn test_function_tool_preserves_description() {
        let mut toolset = ToolSet::new();
        toolset.insert(
            "my_tool".to_string(),
            Tool::function(json!({"type": "object"})).with_description("Does something useful"),
        );

        let (tools, _) = prepare_tools_and_tool_choice(Some(&toolset), None);

        let tools = tools.unwrap();
        if let LanguageModelTool::Function(f) = &tools[0] {
            assert_eq!(f.description, Some("Does something useful".to_string()));
        } else {
            panic!("Expected Function tool");
        }
    }
}
