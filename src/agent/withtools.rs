use std::time::{Duration, Instant};

use serde_json::{Value, json};

use crate::llm::{Client, Error, Provider};
use crate::tools::{Tools, tool_infos};

pub const SYSTEM_MESSAGE: &str = "You are the best coder in the world!";

pub fn is_openai(model: &str) -> bool {
    model.starts_with('o') || model.to_lowercase().contains("gpt")
}

fn add_additional_properties(value: &mut Value) {
    if let Value::Object(map) = value {
        if map.contains_key("properties") {
            map.insert("additionalProperties".to_owned(), Value::Bool(false));
        }
        for child in map.values_mut() {
            add_additional_properties(child);
        }
    }
}

fn strict(info: &Value) -> Value {
    let mut schema = info["input_schema"].clone();
    add_additional_properties(&mut schema);
    let names: Vec<String> = schema["properties"]
        .as_object()
        .map(|p| p.keys().cloned().collect())
        .unwrap_or_default();
    for name in names {
        let required = schema["required"].as_array().cloned().unwrap_or_default();
        if required.iter().any(|r| r == name.as_str()) {
            continue;
        }
        schema["required"] = Value::Array([required, vec![json!(name)]].concat());
        let kind = &mut schema["properties"][&name]["type"];
        *kind = match kind.take() {
            Value::String(s) => json!([s, "null"]),
            Value::Array(mut types) => {
                types.push(json!("null"));
                Value::Array(types)
            }
            other => other,
        };
    }
    json!({
        "type": "function",
        "name": info["name"],
        "description": info["description"],
        "parameters": schema,
        "strict": true,
    })
}

pub fn convert_tool_info(info: &Value, model: &str) -> Value {
    if is_openai(model) && !model.to_lowercase().contains("vllm") {
        return strict(info);
    }
    let properties = &info["input_schema"]["properties"];
    let required: Vec<&String> = properties
        .as_object()
        .map(|p| p.keys().collect())
        .unwrap_or_default();
    json!({
        "type": "function",
        "function": {
            "name": info["name"],
            "description": info["description"],
            "parameters": {
                "type": "object",
                "properties": properties,
                "required": required,
                "additionalProperties": false,
            },
        },
    })
}

fn generic(role: &str, content: Value) -> Value {
    json!({"role": role, "content": content})
}

fn convert_openai(msg: &Value) -> Value {
    match msg {
        Value::Object(map)
            if map.contains_key("role")
                || map.contains_key("content")
                || map.get("type").is_some_and(|t| t == "function_call_output") =>
        {
            let role = map.get("role").cloned().unwrap_or_else(|| json!("user"));
            let content = map.get("content").cloned().unwrap_or_else(|| {
                let output = map
                    .get("output")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                json!(format!("Tool Result: {output}"))
            });
            json!({"role": role, "content": content})
        }
        Value::String(s) => generic("assistant", json!(s)),
        other => generic("assistant", json!(other.to_string())),
    }
}

fn convert_open_router(msg: &Value) -> Value {
    let role = msg["role"].as_str().unwrap_or_default();
    let content = match msg.get("content") {
        Some(Value::String(s)) if role == "tool" => json!(format!("Tool Result: {s}")),
        Some(content) => content.clone(),
        None => {
            let call = &msg["tool_calls"][0];
            let name = call["name"].as_str().or(call["function"]["name"].as_str());
            json!(format!(
                "Function: {}\nArguments: {}",
                name.unwrap_or_default(),
                call["function"]["arguments"].as_str().unwrap_or_default()
            ))
        }
    };
    generic(role, content)
}

pub fn convert_msg_history(history: &[Value], model: &str) -> Vec<Value> {
    let convert = if is_openai(model) {
        convert_openai
    } else {
        convert_open_router
    };
    history.iter().map(convert).collect()
}

#[derive(Debug, Clone, PartialEq)]
pub struct ToolUse {
    pub id: String,
    pub name: String,
    pub input: Value,
}

fn tool_use(id: &Value, name: &Value, arguments: &Value) -> Result<ToolUse, Error> {
    let arguments = arguments.as_str().unwrap_or_default();
    let input = serde_json::from_str(arguments)
        .map_err(|e| Error::Response(format!("{e}: {arguments}")))?;
    Ok(ToolUse {
        id: id.as_str().unwrap_or_default().to_owned(),
        name: name.as_str().unwrap_or_default().to_owned(),
        input,
    })
}

pub fn check_for_tool_use(response: &Value, model: &str) -> Result<Option<ToolUse>, Error> {
    if is_openai(model) {
        let call = response["output"]
            .as_array()
            .and_then(|items| items.iter().find(|i| i["type"] == "function_call"));
        return call
            .map(|c| tool_use(&c["call_id"], &c["name"], &c["arguments"]))
            .transpose();
    }
    match response["choices"][0]["message"]["tool_calls"].as_array() {
        Some(calls) if !calls.is_empty() => {
            let call = &calls[0];
            tool_use(
                &call["id"],
                &call["function"]["name"],
                &call["function"]["arguments"],
            )
            .map(Some)
        }
        _ => Ok(None),
    }
}

#[derive(Debug, Clone)]
pub struct Backoff {
    pub cap: Duration,
    pub budget: Duration,
}

impl Default for Backoff {
    fn default() -> Self {
        Backoff {
            cap: Duration::from_secs(60),
            budget: Duration::from_secs(600),
        }
    }
}

impl Backoff {
    fn wait(&self, attempt: u32) -> Duration {
        let ceiling = Duration::from_secs(1u64 << attempt.min(16)).min(self.cap);
        ceiling.mul_f64(rand::random::<f64>())
    }
}

pub struct Chat<'a> {
    pub client: Client,
    pub tools: Tools,
    pub log: &'a dyn Fn(&str),
    pub max_llm_calls: usize,
    pub timeout: f64,
    pub retries: u32,
    pub backoff: Backoff,
}

impl<'a> Chat<'a> {
    pub fn new(client: Client, log: &'a dyn Fn(&str)) -> Self {
        Chat {
            client,
            tools: Tools::default(),
            log,
            max_llm_calls: 1000,
            timeout: 3600.0,
            retries: 3,
            backoff: Backoff::default(),
        }
    }

    fn openai(&self) -> bool {
        is_openai(&self.client.model)
    }

    fn request(&self, messages: &[Value], tools: &[Value]) -> Result<Value, Error> {
        let mut input = vec![json!({"role": "system", "content": SYSTEM_MESSAGE})];
        input.extend(messages.iter().cloned());
        if self.openai() {
            return self.client.post(
                "responses",
                &json!({
                    "model": self.client.model,
                    "input": input,
                    "tool_choice": "auto",
                    "tools": tools,
                    "parallel_tool_calls": false,
                }),
            );
        }
        let model = match self.client.provider {
            Provider::Vllm => self.client.served_model()?,
            _ => self.client.model.clone(),
        };
        self.client.post(
            "chat/completions",
            &json!({
                "model": model,
                "messages": input,
                "tool_choice": "auto",
                "tools": tools,
                "parallel_tool_calls": false,
            }),
        )
    }

    fn attempts(&self, messages: &[Value], tools: &[Value]) -> Result<Value, Error> {
        let mut retry = 0;
        loop {
            match self.request(messages, tools) {
                Ok(response) => return Ok(response),
                Err(e) => {
                    (self.log)(&format!("Error in get_response_withtools: {e}"));
                    if retry == self.retries {
                        return Err(e);
                    }
                    retry += 1;
                }
            }
        }
    }

    pub fn respond(&self, messages: &[Value], tools: &[Value]) -> Result<Value, Error> {
        let start = Instant::now();
        let mut attempt = 0;
        loop {
            match self.attempts(messages, tools) {
                Err(Error::Status { code: 429, .. }) if start.elapsed() < self.backoff.budget => {
                    std::thread::sleep(self.backoff.wait(attempt));
                    attempt += 1;
                }
                result => return result,
            }
        }
    }

    fn step(
        &self,
        history: &[Value],
        new: &mut Vec<Value>,
        tools: &[Value],
    ) -> Result<bool, Error> {
        let mut messages = history.to_vec();
        messages.extend(new.iter().cloned());
        let response = self.respond(&messages, tools)?;
        (self.log)(&format!("Tool Response: {response}"));
        if self.openai() {
            new.extend(response["output"].as_array().cloned().unwrap_or_default());
        } else {
            let message = &response["choices"][0]["message"];
            if message.is_null() {
                return Err(Error::Response(format!("no message in {response}")));
            }
            new.push(message.clone());
        }
        let Some(call) = check_for_tool_use(&response, &self.client.model)? else {
            return Ok(false);
        };
        let result = self.tools.call(&call.name, &call.input);
        (self.log)(&format!("Tool Used: {}", call.name));
        (self.log)(&format!("Tool Input: {}", call.input));
        (self.log)(&format!("Tool Result: {result}"));
        new.push(if self.openai() {
            json!({"type": "function_call_output", "call_id": call.id, "output": result})
        } else {
            json!({"role": "tool", "tool_call_id": call.id, "name": call.name, "content": result})
        });
        Ok(true)
    }

    pub fn run(&self, msg: &str, history: &[Value]) -> (Vec<Value>, usize) {
        let start = Instant::now();
        let mut new = vec![if self.openai() {
            json!({"role": "user", "content": [{"type": "input_text", "text": msg}]})
        } else {
            json!({"role": "user", "content": msg})
        }];
        let separator = "=".repeat(10);
        (self.log)(&format!(
            "\n{separator} User Instruction {separator}\n{msg}"
        ));
        let tools: Vec<Value> = tool_infos()
            .iter()
            .map(|info| convert_tool_info(info, &self.client.model))
            .collect();
        let mut calls = self.max_llm_calls;
        for i in 0..self.max_llm_calls {
            if self.timeout * 0.9 < start.elapsed().as_secs_f64() {
                (self.log)("Timeout reached, stopping further LLM calls.");
                calls = i;
                break;
            }
            match self.step(history, &mut new, &tools) {
                Ok(true) => {}
                Ok(false) => {
                    calls = i + 1;
                    break;
                }
                Err(e) => {
                    let name = if self.openai() {
                        "openai"
                    } else {
                        "open_router"
                    };
                    (self.log)(&format!("Error in chat_with_agent_{name}: {e}"));
                    break;
                }
            }
        }
        let mut all = history.to_vec();
        all.extend(new);
        (all, calls)
    }
}
