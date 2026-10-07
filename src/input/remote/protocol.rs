//! Phone ↔ glasses protocol, version 1. JSON objects with a `"t"` type tag,
//! one per WebSocket text message. The React Native app implements the
//! other side; `tools/remote.html` is a reference client.
//!
//! Phone → glasses:
//! ```json
//! {"t":"hello","v":1,"device":"Pixel 8","token":"…"}     // reconnect when paired
//! {"t":"pair","v":1,"device":"Pixel 8","code":"482913"}  // first time
//! {"t":"action","a":"left"}          // up down left right select back home menu
//! {"t":"text","op":"insert","s":"cats"}   // op: insert backspace clear submit
//! {"t":"time","unix":1791380730,"utc_offset_minutes":60}
//! ```
//! Glasses → phone:
//! ```json
//! {"t":"welcome","v":1,"name":"ORC-4"}
//! {"t":"paired","token":"…"}                     // store it for "hello"
//! {"t":"text_request","prompt":"Search YouTube","value":"ca"}  // open keyboard
//! {"t":"text_request"}                           // no field focused: close it
//! {"t":"error","code":"bad_code","message":"…"}
//! ```

use serde::{Deserialize, Serialize};

pub const VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "t", rename_all = "snake_case")]
pub enum ClientMsg {
    Hello {
        v: u32,
        device: String,
        token: String,
    },
    Pair {
        v: u32,
        device: String,
        code: String,
    },
    Action {
        a: String,
    },
    Text {
        op: TextOp,
        #[serde(default)]
        s: String,
    },
    Time {
        unix: i64,
        utc_offset_minutes: i32,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextOp {
    Insert,
    Backspace,
    Clear,
    Submit,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "t", rename_all = "snake_case")]
pub enum ServerMsg {
    Welcome {
        v: u32,
        name: String,
    },
    Paired {
        token: String,
    },
    TextRequest {
        #[serde(skip_serializing_if = "Option::is_none")]
        prompt: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        value: Option<String>,
    },
    Error {
        code: ErrorCode,
        message: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    BadMessage,
    BadCode,
    Locked,
    NotPaired,
    Version,
    Busy,
    Disabled,
}

pub fn parse(text: &str) -> Result<ClientMsg, String> {
    serde_json::from_str(text).map_err(|e| e.to_string())
}

impl ServerMsg {
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).expect("server messages always serialize")
    }

    pub fn error(code: ErrorCode, message: &str) -> Self {
        Self::Error {
            code,
            message: message.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_every_client_message() {
        assert_eq!(
            parse(r#"{"t":"pair","v":1,"device":"Pixel","code":"012345"}"#).unwrap(),
            ClientMsg::Pair {
                v: 1,
                device: "Pixel".into(),
                code: "012345".into()
            }
        );
        assert!(matches!(
            parse(r#"{"t":"hello","v":1,"device":"P","token":"ab"}"#),
            Ok(ClientMsg::Hello { .. })
        ));
        assert_eq!(
            parse(r#"{"t":"action","a":"left"}"#).unwrap(),
            ClientMsg::Action { a: "left".into() }
        );
        assert_eq!(
            parse(r#"{"t":"text","op":"backspace"}"#).unwrap(),
            ClientMsg::Text {
                op: TextOp::Backspace,
                s: String::new()
            }
        );
        assert!(matches!(
            parse(r#"{"t":"time","unix":1,"utc_offset_minutes":60}"#),
            Ok(ClientMsg::Time { .. })
        ));
    }

    #[test]
    fn rejects_garbage_and_unknown_types() {
        assert!(parse("not json").is_err());
        assert!(parse(r#"{"t":"format_disk"}"#).is_err());
        assert!(parse(r#"{"t":"text","op":"explode"}"#).is_err());
    }

    #[test]
    fn server_messages_have_the_documented_shape() {
        assert_eq!(
            ServerMsg::Paired { token: "ab".into() }.to_json(),
            r#"{"t":"paired","token":"ab"}"#
        );
        assert_eq!(
            ServerMsg::TextRequest {
                prompt: None,
                value: None
            }
            .to_json(),
            r#"{"t":"text_request"}"#
        );
        assert_eq!(
            ServerMsg::error(ErrorCode::BadCode, "Wrong code").to_json(),
            r#"{"t":"error","code":"bad_code","message":"Wrong code"}"#
        );
    }
}
