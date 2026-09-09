use std::sync::Arc;

use needle_infer::v2_engine::V2Engine;
use serde_json::json;
use tracing::{info, warn};
use zbus::{fdo, interface, object_server::SignalEmitter};

use crate::{
    commands::split_utterance,
    tools::{self, ToolCall, TOOL_SCHEMAS},
};

const CONFIDENCE_THRESHOLD: f32 = 0.70;

pub struct QuinnDaemon {
    engine: Arc<V2Engine>,
}

impl QuinnDaemon {
    pub fn new(engine: Arc<V2Engine>) -> Self {
        Self { engine }
    }

    fn execute_fragment(&self, fragment: &str) -> (String, Option<(ToolCall, String)>) {
        let result = self.engine.run(fragment, TOOL_SCHEMAS);
        if let Some(err) = result.error() {
            return (format!("I couldn't process that command: {err}"), None);
        }

        let Some(raw_call) = result.tool_call.as_deref() else {
            return (
                "I didn't find an action for that command.".to_string(),
                None,
            );
        };

        let confidence = self
            .engine
            .confidence_for(fragment, TOOL_SCHEMAS, &result.text)
            .unwrap_or(0.0);
        if confidence < CONFIDENCE_THRESHOLD {
            warn!(
                query = fragment,
                confidence, "tool call rejected by confidence gate"
            );
            return (
                "I'm not confident enough to run that command.".to_string(),
                None,
            );
        }

        let call = match tools::parse_tool_call(raw_call) {
            Ok(call) => call,
            Err(e) => return (format!("I couldn't understand the tool call: {e}"), None),
        };

        match tools::execute(&call) {
            Ok(message) => ("Done.".to_string(), Some((call, message))),
            Err(message) => (
                format!("I couldn't complete that: {message}"),
                Some((call, message)),
            ),
        }
    }
}

#[interface(name = "org.quinn.Assistant", spawn = true)]
impl QuinnDaemon {
    #[zbus(property, name = "Ready")]
    fn ready(&self) -> bool {
        true
    }

    #[zbus(out_args("response", "tool_calls"))]
    async fn ask(
        &self,
        query: &str,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> fdo::Result<(String, String)> {
        let query = query.trim();
        if query.is_empty() {
            return Ok((
                "Tell me what you want me to do.".to_string(),
                "[]".to_string(),
            ));
        }

        let mut responses = Vec::new();
        let mut executed = Vec::new();

        for fragment in split_utterance(query) {
            let (response, event) = self.execute_fragment(&fragment);
            responses.push(response);
            if let Some((call, result)) = event {
                let event_json = tools::event_json(&call, &result);
                if let Err(e) = emitter
                    .tool_executed(&call.name, &call.arguments.to_string(), &result)
                    .await
                {
                    warn!(error = %e, "failed to emit ToolExecuted signal");
                }
                executed.push(event_json);
            }
        }

        let response = if responses.len() == 1 {
            responses.remove(0)
        } else {
            responses.join(" ")
        };
        let tool_calls = json!(executed).to_string();
        info!(query, "handled request");
        Ok((response, tool_calls))
    }

    #[zbus(signal)]
    async fn tool_executed(
        emitter: &SignalEmitter<'_>,
        name: &str,
        args: &str,
        result: &str,
    ) -> zbus::Result<()>;
}
