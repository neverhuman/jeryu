use crate::llm::ChatMessage;
use serde::{Deserialize, Serialize};

#[derive(Serialize)]
pub(crate) struct ChatRequest<'a> {
    pub model: &'a str,
    pub messages: &'a [ChatMessage],
    pub temperature: f32,
    pub max_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seed: Option<u64>,
}

#[derive(Deserialize, Debug)]
pub(crate) struct ChatResponse {
    #[serde(default)]
    pub model: Option<String>,
    pub choices: Vec<Choice>,
    #[serde(default)]
    pub usage: Option<Usage>,
}

#[derive(Deserialize, Debug)]
pub(crate) struct Choice {
    pub message: ChoiceMessage,
}

#[derive(Deserialize, Debug)]
pub(crate) struct ChoiceMessage {
    #[serde(default)]
    pub content: Option<String>,
}

#[derive(Deserialize, Debug, Default)]
pub(crate) struct Usage {
    #[serde(default)]
    pub prompt_tokens: Option<u32>,
    #[serde(default)]
    pub completion_tokens: Option<u32>,
}
