use crate::failure::Failure;
use serde_json::{Value, json};
use std::collections::VecDeque;
use std::net::TcpStream;
use std::time::{Duration, Instant};
use tungstenite::stream::MaybeTlsStream;
use tungstenite::{Message, WebSocket};

pub const CALL_TIMEOUT: Duration = Duration::from_secs(30);
pub const POLL_INTERVAL: Duration = Duration::from_millis(250);

const READ_SLICE: Duration = Duration::from_millis(200);
const MAX_QUEUED_EVENTS: usize = 256;

fn protocol<T: std::fmt::Display>(error: T) -> Failure {
    Failure::Protocol(error.to_string())
}

pub struct Connection {
    socket: WebSocket<MaybeTlsStream<TcpStream>>,
    events: VecDeque<Value>,
    next_id: u64,
}

impl Connection {
    pub fn connect(websocket_url: &str) -> Result<Self, Failure> {
        let (socket, _) = tungstenite::connect(websocket_url).map_err(protocol)?;
        let connection = Self {
            socket,
            events: VecDeque::new(),
            next_id: 0,
        };
        connection.set_read_timeout(READ_SLICE)?;
        Ok(connection)
    }

    fn set_read_timeout(&self, timeout: Duration) -> Result<(), Failure> {
        match self.socket.get_ref() {
            MaybeTlsStream::Plain(stream) => stream
                .set_read_timeout(Some(timeout))
                .map_err(|error| Failure::Io(error.to_string())),

            _ => Ok(()),
        }
    }

    pub fn call(
        &mut self,
        session: Option<&str>,
        method: &str,
        params: Value,
        timeout: Duration,
    ) -> Result<Value, Failure> {
        self.next_id += 1;
        let id = self.next_id;
        let mut request = json!({ "id": id, "method": method, "params": params });
        if let Some(session) = session {
            request["sessionId"] = json!(session);
        }
        self.socket
            .send(Message::Text(request.to_string().into()))
            .map_err(protocol)?;

        let deadline = Instant::now() + timeout;
        loop {
            let Some(message) = self.read_one(deadline)? else {
                return Err(Failure::Timeout(method.to_string()));
            };
            if message.get("id").and_then(Value::as_u64) != Some(id) {
                self.file_event(message);
                continue;
            }
            if let Some(error) = message.get("error") {
                let text = error
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown DevTools error");
                return Err(Failure::Protocol(format!("{method}: {text}")));
            }
            return Ok(message.get("result").cloned().unwrap_or(Value::Null));
        }
    }

    fn file_event(&mut self, message: Value) {
        if message.get("method").is_some() {
            if self.events.len() >= MAX_QUEUED_EVENTS {
                self.events.pop_front();
            }
            self.events.push_back(message);
        }
    }

    pub fn wait_for_event(&mut self, method: &str, timeout: Duration) -> Result<Value, Failure> {
        if let Some(index) = self
            .events
            .iter()
            .position(|event| event.get("method").and_then(Value::as_str) == Some(method))
        {
            return Ok(self.events.remove(index).unwrap_or(Value::Null));
        }
        let deadline = Instant::now() + timeout;
        loop {
            let Some(message) = self.read_one(deadline)? else {
                return Err(Failure::Timeout(method.to_string()));
            };
            if message.get("method").and_then(Value::as_str) == Some(method) {
                return Ok(message);
            }
            self.file_event(message);
        }
    }

    fn read_one(&mut self, deadline: Instant) -> Result<Option<Value>, Failure> {
        loop {
            if Instant::now() >= deadline {
                return Ok(None);
            }
            match self.socket.read() {
                Ok(Message::Text(text)) => {
                    return serde_json::from_str(text.as_str())
                        .map(Some)
                        .map_err(protocol);
                }
                Ok(Message::Close(_)) => {
                    return Err(Failure::Protocol(
                        "the browser closed the connection".to_string(),
                    ));
                }
                Ok(_) => continue,
                Err(tungstenite::Error::Io(error))
                    if matches!(
                        error.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                    ) =>
                {
                    continue;
                }
                Err(error) => return Err(protocol(error)),
            }
        }
    }

    pub fn user_agent(&mut self) -> Result<String, Failure> {
        let version = self.call(None, "Browser.getVersion", json!({}), CALL_TIMEOUT)?;
        Ok(version
            .get("userAgent")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string())
    }

    pub fn close_browser(&mut self) {
        self.next_id += 1;
        let request = json!({ "id": self.next_id, "method": "Browser.close", "params": {} });
        let _ = self.socket.send(Message::Text(request.to_string().into()));
        let _ = self.socket.flush();
    }

    pub fn open(&mut self, url: &str) -> Result<Page, Failure> {
        let target = self.call(
            None,
            "Target.createTarget",
            json!({ "url": url }),
            CALL_TIMEOUT,
        )?;
        let target_id = target
            .get("targetId")
            .and_then(Value::as_str)
            .ok_or_else(|| Failure::Protocol("Target.createTarget returned no id".to_string()))?
            .to_string();

        let attached = self.call(
            None,
            "Target.attachToTarget",
            json!({ "targetId": target_id, "flatten": true }),
            CALL_TIMEOUT,
        )?;
        let session = attached
            .get("sessionId")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                Failure::Protocol("Target.attachToTarget returned no session".to_string())
            })?
            .to_string();

        let mut page = Page { session, target_id };
        page.enable(self)?;
        Ok(page)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    Enter,
    Tab,
    Escape,
}

impl Key {
    fn descriptor(self) -> (&'static str, i64, &'static str) {
        match self {
            Self::Enter => ("Enter", 13, "\r"),
            Self::Tab => ("Tab", 9, "\t"),
            Self::Escape => ("Escape", 27, ""),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Page {
    pub session: String,
    pub target_id: String,
}

impl Page {
    fn enable(&mut self, connection: &mut Connection) -> Result<(), Failure> {
        for domain in ["Page", "Runtime", "DOM"] {
            connection.call(
                Some(&self.session),
                &format!("{domain}.enable"),
                json!({}),
                CALL_TIMEOUT,
            )?;
        }
        Ok(())
    }

    pub fn call(
        &self,
        connection: &mut Connection,
        method: &str,
        params: Value,
    ) -> Result<Value, Failure> {
        connection.call(Some(&self.session), method, params, CALL_TIMEOUT)
    }

    pub fn set_user_agent(&self, connection: &mut Connection, agent: &str) -> Result<(), Failure> {
        self.call(
            connection,
            "Emulation.setUserAgentOverride",
            json!({ "userAgent": agent }),
        )?;
        Ok(())
    }

    pub fn navigate(&self, connection: &mut Connection, url: &str) -> Result<(), Failure> {
        let result = self.call(connection, "Page.navigate", json!({ "url": url }))?;
        if let Some(text) = result.get("errorText").and_then(Value::as_str) {
            return Err(Failure::Protocol(format!("{url}: {text}")));
        }
        Ok(())
    }

    pub fn eval(&self, connection: &mut Connection, script: &str) -> Result<Value, Failure> {
        let result = self.call(
            connection,
            "Runtime.evaluate",
            json!({
                "expression": script,
                "returnByValue": true,
                "awaitPromise": true,
                "userGesture": true,
            }),
        )?;

        if let Some(details) = result.get("exceptionDetails") {
            let text = details
                .get("exception")
                .and_then(|exception| exception.get("description"))
                .and_then(Value::as_str)
                .or_else(|| details.get("text").and_then(Value::as_str))
                .unwrap_or("script threw");
            return Err(Failure::Protocol(text.to_string()));
        }
        Ok(result
            .get("result")
            .and_then(|value| value.get("value"))
            .cloned()
            .unwrap_or(Value::Null))
    }

    pub fn eval_bool(&self, connection: &mut Connection, script: &str) -> Result<bool, Failure> {
        Ok(self.eval(connection, script)? == Value::Bool(true))
    }

    pub fn eval_string(
        &self,
        connection: &mut Connection,
        script: &str,
    ) -> Result<String, Failure> {
        Ok(self
            .eval(connection, script)?
            .as_str()
            .unwrap_or_default()
            .to_string())
    }

    pub fn url(&self, connection: &mut Connection) -> Result<String, Failure> {
        self.eval_string(connection, "location.href")
    }

    pub fn wait_until(
        &self,
        connection: &mut Connection,
        script: &str,
        timeout: Duration,
        what: &str,
    ) -> Result<(), Failure> {
        let deadline = Instant::now() + timeout;
        loop {
            if self.eval_bool(connection, script).unwrap_or(false) {
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err(Failure::Timeout(what.to_string()));
            }
            std::thread::sleep(POLL_INTERVAL);
        }
    }

    pub fn race(
        &self,
        connection: &mut Connection,
        options: &[&str],
        timeout: Duration,
        what: &str,
    ) -> Result<usize, Failure> {
        let deadline = Instant::now() + timeout;
        loop {
            for (index, script) in options.iter().enumerate() {
                if self.eval_bool(connection, script).unwrap_or(false) {
                    return Ok(index);
                }
            }
            if Instant::now() >= deadline {
                return Err(Failure::Timeout(what.to_string()));
            }
            std::thread::sleep(POLL_INTERVAL);
        }
    }

    pub fn focus(&self, connection: &mut Connection, selector: &str) -> Result<(), Failure> {
        let found = self.eval_bool(
            connection,
            &format!(
                "(() => {{ const node = {}; if (!node) return false; node.focus(); return true; }})()",
                query(selector)
            ),
        )?;
        if found {
            Ok(())
        } else {
            Err(Failure::PageChanged(selector.to_string()))
        }
    }

    pub fn screenshot(&self, connection: &mut Connection, to: &std::path::Path) -> bool {
        let Ok(answer) = self.call(
            connection,
            "Page.captureScreenshot",
            json!({ "format": "png" }),
        ) else {
            return false;
        };
        let Some(data) = answer.get("data").and_then(|value| value.as_str()) else {
            return false;
        };
        let Some(bytes) = crate::login::decode_base64(data) else {
            return false;
        };
        std::fs::write(to, bytes).is_ok()
    }

    pub fn pretend_to_be_focused(&self, connection: &mut Connection) {
        let _ = self.call(connection, "Page.bringToFront", json!({}));
        let _ = self.call(
            connection,
            "Emulation.setFocusEmulationEnabled",
            json!({ "enabled": true }),
        );
    }

    pub fn click_point(&self, connection: &mut Connection, x: f64, y: f64) -> Result<(), Failure> {
        for kind in ["mousePressed", "mouseReleased"] {
            self.call(
                connection,
                "Input.dispatchMouseEvent",
                json!({
                    "type": kind,
                    "x": x,
                    "y": y,
                    "button": "left",
                    "buttons": 1,
                    "clickCount": 1,
                }),
            )?;
        }
        Ok(())
    }

    pub fn insert_text(&self, connection: &mut Connection, text: &str) -> Result<(), Failure> {
        self.call(connection, "Input.insertText", json!({ "text": text }))?;
        Ok(())
    }

    pub fn fill(
        &self,
        connection: &mut Connection,
        selector: &str,
        text: &str,
    ) -> Result<(), Failure> {
        self.focus(connection, selector)?;
        self.clear_focused(connection)?;
        self.insert_text(connection, text)
    }

    fn clear_focused(&self, connection: &mut Connection) -> Result<(), Failure> {
        self.eval(
            connection,
            "(() => { const node = document.activeElement; if (!node) return; \
             if (node.isContentEditable) { document.execCommand('selectAll', false, null); \
             document.execCommand('delete', false, null); } \
             else if ('value' in node) { node.select ? node.select() : null; node.value = ''; \
             node.dispatchEvent(new Event('input', { bubbles: true })); } })()",
        )?;
        Ok(())
    }

    pub fn press(&self, connection: &mut Connection, key: Key) -> Result<(), Failure> {
        let (name, code, text) = key.descriptor();
        for kind in ["rawKeyDown", "keyUp"] {
            let mut params = json!({
                "type": kind,
                "key": name,
                "code": name,
                "windowsVirtualKeyCode": code,
                "nativeVirtualKeyCode": code,
            });
            if kind == "rawKeyDown" && !text.is_empty() {
                params["text"] = json!(text);
            }
            self.call(connection, "Input.dispatchKeyEvent", params)?;
        }
        Ok(())
    }

    pub fn click(&self, connection: &mut Connection, selector: &str) -> Result<(), Failure> {
        let clicked = self.eval_bool(
            connection,
            &format!(
                "(() => {{ const node = {}; if (!node) return false; node.click(); return true; }})()",
                query(selector)
            ),
        )?;
        if clicked {
            Ok(())
        } else {
            Err(Failure::PageChanged(selector.to_string()))
        }
    }

    pub fn choose_file(
        &self,
        connection: &mut Connection,
        button: &str,
        path: &std::path::Path,
    ) -> Result<(), Failure> {
        self.call(
            connection,
            "Page.setInterceptFileChooserDialog",
            json!({ "enabled": true }),
        )?;

        let clicked = self.click(connection, button);
        let opened = clicked
            .and_then(|()| connection.wait_for_event("Page.fileChooserOpened", CALL_TIMEOUT));

        let restored = self.call(
            connection,
            "Page.setInterceptFileChooserDialog",
            json!({ "enabled": false }),
        );

        let opened = opened?;
        restored?;

        let node = opened
            .get("params")
            .and_then(|params| params.get("backendNodeId"))
            .and_then(Value::as_u64)
            .ok_or_else(|| Failure::Protocol("the file chooser named no element".to_string()))?;

        self.call(
            connection,
            "DOM.setFileInputFiles",
            json!({ "backendNodeId": node, "files": [path.to_string_lossy()] }),
        )?;
        Ok(())
    }

    pub fn set_file(
        &self,
        connection: &mut Connection,
        selector: &str,
        path: &std::path::Path,
    ) -> Result<(), Failure> {
        let resolved = self.call(
            connection,
            "Runtime.evaluate",
            json!({ "expression": query(selector), "returnByValue": false }),
        )?;
        let object_id = resolved
            .get("result")
            .and_then(|result| result.get("objectId"))
            .and_then(Value::as_str)
            .ok_or_else(|| Failure::PageChanged(selector.to_string()))?
            .to_string();

        self.call(
            connection,
            "DOM.setFileInputFiles",
            json!({ "objectId": object_id, "files": [path.to_string_lossy()] }),
        )?;
        Ok(())
    }

    pub fn close(&self, connection: &mut Connection) -> Result<(), Failure> {
        connection.call(
            None,
            "Target.closeTarget",
            json!({ "targetId": self.target_id }),
            CALL_TIMEOUT,
        )?;
        Ok(())
    }
}

pub fn visible_user_agent(agent: &str) -> String {
    agent.replace("HeadlessChrome", "Chrome")
}

pub fn is_headless_agent(agent: &str) -> bool {
    agent.contains("HeadlessChrome")
}

pub fn query(selector: &str) -> String {
    format!(
        "(() => {{ const wanted = {}; const seen = new Set();          const walk = (root) => {{ if (!root || seen.has(root)) return null; seen.add(root);          const hit = root.querySelector(wanted); if (hit) return hit;          for (const node of root.querySelectorAll('*')) {{          if (node.shadowRoot) {{ const found = walk(node.shadowRoot); if (found) return found; }} }}          return null; }}; return walk(document); }})()",
        js_string(selector)
    )
}

pub fn query_all(selector: &str) -> String {
    format!(
        "(() => {{ const wanted = {}; const seen = new Set(); const found = [];          const walk = (root) => {{ if (!root || seen.has(root)) return; seen.add(root);          for (const hit of root.querySelectorAll(wanted)) found.push(hit);          for (const node of root.querySelectorAll('*')) if (node.shadowRoot) walk(node.shadowRoot);          }}; walk(document); return found; }})()",
        js_string(selector)
    )
}

pub fn js_string(value: &str) -> String {
    Value::String(value.to_string())
        .to_string()
        .replace('\u{2028}', "\\u2028")
        .replace('\u{2029}', "\\u2029")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_hidden_window_stops_announcing_itself_as_one() {
        let headless = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, \
                        like Gecko) HeadlessChrome/140.0.0.0 Safari/537.36";
        assert!(is_headless_agent(headless));

        let fixed = visible_user_agent(headless);
        assert!(!is_headless_agent(&fixed));
        assert!(fixed.contains("Chrome/140.0.0.0"), "{fixed}");
        assert!(
            fixed.contains("Windows NT 10.0"),
            "the platform is left alone"
        );
        assert!(fixed.contains("Safari/537.36"));
    }

    #[test]
    fn an_ordinary_agent_is_left_exactly_as_it_is() {
        let ordinary = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, \
                        like Gecko) Chrome/140.0.0.0 Safari/537.36";
        assert!(!is_headless_agent(ordinary));
        assert_eq!(visible_user_agent(ordinary), ordinary);
        assert_eq!(visible_user_agent(""), "");
    }

    #[test]
    fn a_selector_is_quoted_rather_than_pasted_into_the_script() {
        assert!(query("#id").contains("\"#id\""));
        assert!(query("input[name=\"q\"]").contains("input[name=\\\"q\\\"]"));
        assert!(query_all("#id").contains("\"#id\""));
    }

    #[test]
    fn a_lookup_walks_into_shadow_roots_because_studio_is_web_components() {
        for script in [query("#id"), query_all("#id")] {
            assert!(script.contains("shadowRoot"), "{script}");
            assert!(script.contains("querySelectorAll('*')"), "{script}");
        }
    }

    #[test]
    fn a_cyclic_or_repeated_root_cannot_send_the_walk_round_for_ever() {
        for script in [query("#id"), query_all("#id")] {
            assert!(script.contains("seen"), "{script}");
        }
    }

    #[test]
    fn a_string_with_quotes_newlines_or_line_separators_cannot_break_out() {
        assert_eq!(js_string("a\"b"), "\"a\\\"b\"");
        assert_eq!(js_string("line\nbreak"), "\"line\\nbreak\"");
        assert_eq!(js_string("back\\slash"), "\"back\\\\slash\"");
        assert_eq!(js_string("u\u{2028}v"), "\"u\\u2028v\"");
        assert_eq!(js_string("u\u{2029}v"), "\"u\\u2029v\"");
        assert!(!js_string("</script>").contains('\n'));
    }

    #[test]
    fn a_title_full_of_punctuation_survives_the_round_trip_into_a_script() {
        for title in [
            "My video: part 2 \"final\"",
            "back\\slash and 'quotes'",
            "emoji 🎬 and a\ttab",
            "русский заголовок",
        ] {
            let literal = js_string(title);
            let parsed: String = serde_json::from_str(&literal).expect("valid JSON string");
            assert_eq!(parsed, title);
        }
    }

    #[test]
    fn the_keys_carry_the_codes_a_browser_expects() {
        assert_eq!(Key::Enter.descriptor(), ("Enter", 13, "\r"));
        assert_eq!(Key::Tab.descriptor(), ("Tab", 9, "\t"));
        assert_eq!(Key::Escape.descriptor().1, 27);
        assert_eq!(Key::Escape.descriptor().2, "", "escape inserts no text");
    }
}
