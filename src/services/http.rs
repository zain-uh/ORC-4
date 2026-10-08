//! HTTPS GET for worker jobs, with timeouts and a response size cap.
//!
//! Device (Linux): ureq with rustls and bundled root certificates.
//! Dev PC (Windows): the system `curl.exe` (Windows 10+), which verifies
//! certificates with Windows' own TLS. This keeps the Windows build free of
//! C compilers and import-library tools; the device path is the product one.

use super::UserError;

/// Identifies ORC-4 to servers, with a contact URL (Wikimedia's API policy
/// asks for one).
pub const USER_AGENT: &str = concat!(
    "ORC-4/",
    env!("CARGO_PKG_VERSION"),
    " (smart glasses; https://github.com/iNoExist/ORC-4)"
);

/// Downloads `url`, refusing bodies larger than `max_bytes`.
pub fn get(url: &str, max_bytes: u64) -> Result<Vec<u8>, UserError> {
    imp::get(url, max_bytes)
}

#[cfg(not(windows))]
mod imp {
    use std::sync::OnceLock;
    use std::time::Duration;

    use ureq::Agent;
    use ureq::config::Config;

    use super::UserError;

    fn agent() -> &'static Agent {
        static AGENT: OnceLock<Agent> = OnceLock::new();
        AGENT.get_or_init(|| {
            let config = Config::builder()
                .timeout_connect(Some(Duration::from_secs(5)))
                .timeout_global(Some(Duration::from_secs(15)))
                .user_agent(super::USER_AGENT)
                .build();
            Agent::new_with_config(config)
        })
    }

    pub fn get(url: &str, max_bytes: u64) -> Result<Vec<u8>, UserError> {
        let mut response = agent().get(url).call().map_err(network_error)?;
        response
            .body_mut()
            .with_config()
            .limit(max_bytes)
            .read_to_vec()
            .map_err(network_error)
    }

    fn network_error(e: ureq::Error) -> UserError {
        log::info!("http: {e}");
        match e {
            ureq::Error::StatusCode(404) => UserError::NotFound,
            ureq::Error::BodyExceedsLimit(_) => UserError::TooLarge,
            _ => UserError::Network,
        }
    }
}

#[cfg(windows)]
mod imp {
    use std::process::{Command, Stdio};

    use super::UserError;

    pub fn get(url: &str, max_bytes: u64) -> Result<Vec<u8>, UserError> {
        let output = Command::new("curl.exe")
            .args(["--silent", "--show-error", "--fail", "--location"])
            .args(["--connect-timeout", "5", "--max-time", "15"])
            .args(["--max-filesize", &max_bytes.to_string()])
            .args(["--user-agent", super::USER_AGENT])
            .arg("--")
            .arg(url)
            .stdin(Stdio::null())
            .output()
            .map_err(|e| {
                log::warn!("http: cannot run curl.exe: {e}");
                UserError::Network
            })?;
        match output.status.code() {
            Some(0) if output.stdout.len() as u64 <= max_bytes => Ok(output.stdout),
            Some(0) | Some(63) => Err(UserError::TooLarge), // 63: max filesize exceeded
            _ => {
                log::info!("http: {}", String::from_utf8_lossy(&output.stderr).trim());
                Err(UserError::Network)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unreachable_host_is_a_network_error() {
        // Nothing listens on port 9 locally: fails fast without the internet.
        assert_eq!(
            get("http://127.0.0.1:9/x.jpg", 1024).unwrap_err(),
            UserError::Network
        );
    }
}

/// Percent-encodes `s` for use in a URL path segment or query value.
pub fn encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

#[cfg(test)]
mod encode_tests {
    use super::encode;

    #[test]
    fn encodes_everything_but_unreserved_characters() {
        assert_eq!(encode("Rust (language)"), "Rust%20%28language%29");
        assert_eq!(encode("a&b=c/d?"), "a%26b%3Dc%2Fd%3F");
        assert_eq!(encode("Zoë"), "Zo%C3%AB");
        assert_eq!(encode("safe-_.~"), "safe-_.~");
    }
}
