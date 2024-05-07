use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum STTRequest {
    RegisterApiKey(String),
    OpenaiTranscribe(Vec<u8>),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum STTResponse {
    Ok,
    OpenaiTranscribed(String),
    Error(String),
}

