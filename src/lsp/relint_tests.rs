//! End-to-end tests for re-linting open documents after a workspace index change.
//!
//! These drive a real `LspService` over an in-memory duplex stream and read the
//! frames the server writes, because the defect they cover is the absence of a
//! message: the server can compute the right diagnostics and simply never tell
//! the editor. A test that calls a server method directly asks for the answer,
//! and so cannot see whether anything was ever sent.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader, DuplexStream, ReadHalf, WriteHalf};
use tokio::sync::Mutex;
use tower_lsp::lsp_types::Url;
use tower_lsp::{LspService, Server};

use crate::lsp::server::RumdlLanguageServer;
use crate::lsp::types::IndexState;

/// How long a test waits for a message that should arrive within a debounce
/// interval or two. Generous, because the failure it guards is a message that
/// never comes at all rather than one that comes late.
const WAIT_TIMEOUT: Duration = Duration::from_secs(10);

/// A client speaking LSP to a real server over an in-memory pipe.
struct LspTestClient {
    writer: Arc<Mutex<WriteHalf<DuplexStream>>>,
    received: Arc<Mutex<Vec<Value>>>,
    next_id: AtomicI64,
    /// The server itself, for the few assertions about state rather than
    /// messages (waiting for the index to finish building).
    server: RumdlLanguageServer,
}

impl LspTestClient {
    /// Start a server whose configuration is exactly `config_path`.
    ///
    /// Passed as the `--config` path rather than left to discovery so the test
    /// cannot pick up a config from the machine it runs on.
    fn start(config_path: &Path) -> Self {
        let config_path = config_path.to_string_lossy().into_owned();
        let (service, socket) = LspService::new(move |client| RumdlLanguageServer::new(client, Some(&config_path)));
        let server = service.inner().clone();

        let (client_end, server_end) = tokio::io::duplex(1024 * 1024);
        let (server_read, server_write) = tokio::io::split(server_end);
        tokio::spawn(Server::new(server_read, server_write, socket).serve(service));

        let (client_read, client_write) = tokio::io::split(client_end);
        let writer = Arc::new(Mutex::new(client_write));
        let received = Arc::new(Mutex::new(Vec::new()));
        tokio::spawn(read_loop(BufReader::new(client_read), writer.clone(), received.clone()));

        Self {
            writer,
            received,
            next_id: AtomicI64::new(1),
            server,
        }
    }

    async fn notify(&self, method: &str, params: Value) {
        write_frame(
            &self.writer,
            &json!({"jsonrpc": "2.0", "method": method, "params": params}),
        )
        .await;
    }

    /// Send a request and wait for its response.
    async fn request(&self, method: &str, params: Value) -> Value {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        write_frame(
            &self.writer,
            &json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}),
        )
        .await;
        self.wait_for(&format!("a response to {method}"), |messages| {
            messages
                .iter()
                .find(|m| m.get("method").is_none() && m.get("id").and_then(Value::as_i64) == Some(id))
                .cloned()
        })
        .await
    }

    /// Poll the received messages until `predicate` answers, or fail the test.
    ///
    /// Reads the whole history each time rather than a stream position, so a
    /// message that arrived before the wait started still counts: these tests
    /// ask whether something was ever sent, not when.
    async fn wait_for<T>(&self, what: &str, mut predicate: impl FnMut(&[Value]) -> Option<T>) -> T {
        let deadline = Instant::now() + WAIT_TIMEOUT;
        loop {
            {
                let messages = self.received.lock().await;
                if let Some(found) = predicate(&messages) {
                    return found;
                }
                if Instant::now() >= deadline {
                    let seen: Vec<String> = messages
                        .iter()
                        .map(|m| match (m.get("method").and_then(Value::as_str), publish_uri(m)) {
                            (Some(method), Some(uri)) => format!("{method} {uri} {:?}", publish_messages(m)),
                            (Some(method), None) => method.to_string(),
                            (None, _) => "<response>".to_string(),
                        })
                        .collect();
                    panic!("timed out waiting for {what}; server sent: {seen:#?}");
                }
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    /// Block until the background index worker reports the workspace indexed.
    ///
    /// Panics rather than returning on timeout: every assertion after it is
    /// meaningless against a half-built index, so proceeding would turn a
    /// stalled worker into a confident wrong answer.
    async fn wait_for_index_ready(&self) {
        let deadline = Instant::now() + WAIT_TIMEOUT;
        while !matches!(*self.server.index_state.read().await, IndexState::Ready) {
            assert!(Instant::now() < deadline, "workspace index never became ready");
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    async fn initialize(&self, root: &Path, capabilities: Value) {
        let root_uri = Url::from_directory_path(root).unwrap();
        self.request(
            "initialize",
            json!({
                "processId": Value::Null,
                "rootUri": root_uri,
                "capabilities": capabilities,
                "workspaceFolders": [{"uri": root_uri, "name": "test"}],
            }),
        )
        .await;
    }

    async fn did_open(&self, path: &Path, text: &str) {
        self.notify(
            "textDocument/didOpen",
            json!({"textDocument": {
                "uri": Url::from_file_path(path).unwrap(),
                "languageId": "markdown",
                "version": 1,
                "text": text,
            }}),
        )
        .await;
    }

    async fn did_change(&self, path: &Path, version: i64, text: &str) {
        self.notify(
            "textDocument/didChange",
            json!({
                "textDocument": {"uri": Url::from_file_path(path).unwrap(), "version": version},
                "contentChanges": [{"text": text}],
            }),
        )
        .await;
    }

    /// Wait until the server has published diagnostics for `path` at least
    /// `count` times, and answer with every publish's messages in order.
    async fn wait_for_publishes(&self, path: &Path, count: usize) -> Vec<Vec<String>> {
        let uri = Url::from_file_path(path).unwrap().to_string();
        let what = format!("{count} publishDiagnostics for {}", path.display());
        self.wait_for(&what, |messages| {
            let publishes: Vec<Vec<String>> = messages
                .iter()
                .filter(|m| m.get("method").and_then(Value::as_str) == Some("textDocument/publishDiagnostics"))
                .filter(|m| publish_uri(m).as_deref() == Some(uri.as_str()))
                .map(publish_messages)
                .collect();
            (publishes.len() >= count).then_some(publishes)
        })
        .await
    }

    /// Wait until a publish for `path` carries a diagnostic matching `needle`.
    async fn wait_for_diagnostic(&self, path: &Path, needle: &str) -> Vec<String> {
        let uri = Url::from_file_path(path).unwrap().to_string();
        let what = format!("a diagnostic matching {needle:?} for {}", path.display());
        self.wait_for(&what, |messages| {
            messages
                .iter()
                .filter(|m| m.get("method").and_then(Value::as_str) == Some("textDocument/publishDiagnostics"))
                .filter(|m| publish_uri(m).as_deref() == Some(uri.as_str()))
                .map(publish_messages)
                .find(|diagnostics| diagnostics.iter().any(|d| d.contains(needle)))
        })
        .await
    }
}

fn publish_uri(message: &Value) -> Option<String> {
    message
        .pointer("/params/uri")
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn publish_messages(message: &Value) -> Vec<String> {
    message
        .pointer("/params/diagnostics")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|d| d.get("message").and_then(Value::as_str))
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

async fn read_loop(
    mut reader: BufReader<ReadHalf<DuplexStream>>,
    writer: Arc<Mutex<WriteHalf<DuplexStream>>>,
    received: Arc<Mutex<Vec<Value>>>,
) {
    while let Some(message) = read_frame(&mut reader).await {
        // A server-to-client request blocks its sender until it is answered, and
        // the index worker's progress reporting is one of them: a client that
        // never answers stalls the very thing these tests measure.
        if let (Some(id), Some(method)) = (
            message.get("id").cloned(),
            message.get("method").and_then(Value::as_str),
        ) {
            let result = if method == "workspace/configuration" {
                let items = message.pointer("/params/items").and_then(Value::as_array);
                Value::Array(vec![Value::Null; items.map_or(1, Vec::len)])
            } else {
                Value::Null
            };
            write_frame(&writer, &json!({"jsonrpc": "2.0", "id": id, "result": result})).await;
        }
        received.lock().await.push(message);
    }
}

async fn read_frame(reader: &mut BufReader<ReadHalf<DuplexStream>>) -> Option<Value> {
    let mut length = 0usize;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).await.ok()? == 0 {
            return None;
        }
        let line = line.trim_end();
        if line.is_empty() {
            break;
        }
        if let Some(value) = line.strip_prefix("Content-Length:") {
            length = value.trim().parse().ok()?;
        }
    }
    let mut body = vec![0u8; length];
    reader.read_exact(&mut body).await.ok()?;
    serde_json::from_slice(&body).ok()
}

async fn write_frame(writer: &Arc<Mutex<WriteHalf<DuplexStream>>>, message: &Value) {
    let body = serde_json::to_vec(message).expect("a test message always serializes");
    let mut writer = writer.lock().await;
    writer
        .write_all(format!("Content-Length: {}\r\n\r\n", body.len()).as_bytes())
        .await
        .expect("the server end of the pipe is still open");
    writer.write_all(&body).await.expect("the server end is still open");
    writer.flush().await.expect("the server end is still open");
}

/// A client that takes diagnostics as they are published.
fn push_capabilities() -> Value {
    json!({"textDocument": {"publishDiagnostics": {}}})
}

/// A client that asks for diagnostics instead, as VS Code does.
fn pull_capabilities() -> Value {
    json!({
        "textDocument": {"publishDiagnostics": {}, "diagnostic": {"dynamicRegistration": false}},
        "workspace": {"diagnostic": {"refreshSupport": true}},
    })
}

/// Write a workspace and answer with its resolved root.
///
/// Resolved the way the server resolves it, because a document URI is matched
/// against index keys built from this root. On macOS the temp directory is
/// reached through a symlink, so the unresolved spelling names paths the index
/// never holds.
fn write_workspace(temp: &tempfile::TempDir, files: &[(&str, &str)]) -> PathBuf {
    for (name, content) in files {
        std::fs::write(temp.path().join(name), content).unwrap();
    }
    crate::lsp::resolve_workspace_root(temp.path())
}

const ENABLE_MD051: &str = "[global]\nenable = [\"MD051\"]\n";

/// Issue #792: a document opened while the workspace index is still building is
/// linted without any cross-file check, and nothing recomputes it once the
/// index is ready. The reporter saw a link diagnostic appear only after making
/// an unrelated edit.
///
/// The index is definitively still building here: the rescan is triggered by
/// `initialized`, which this test sends after the document is open and its
/// first diagnostics have arrived.
#[tokio::test]
async fn test_open_document_is_relinted_when_the_index_becomes_ready() {
    let temp = tempfile::tempdir().unwrap();
    let root = write_workspace(
        &temp,
        &[
            (".rumdl.toml", ENABLE_MD051),
            ("a.md", "# Heading\n"),
            ("b.md", "# B\n\nSee [target](./a.md#missing).\n"),
        ],
    );
    let b = root.join("b.md");

    let client = LspTestClient::start(&root.join(".rumdl.toml"));
    client.initialize(&root, push_capabilities()).await;
    client.did_open(&b, "# B\n\nSee [target](./a.md#missing).\n").await;

    let first = client.wait_for_publishes(&b, 1).await;
    assert!(
        !first[0].iter().any(|d| d.contains("Link fragment")),
        "control: with the index still building the cross-file answer is not available yet, \
         so a later one proves the index becoming ready produced it, got {first:?}"
    );

    client.notify("initialized", json!({})).await;

    let diagnostics = client.wait_for_diagnostic(&b, "Link fragment").await;
    assert!(
        diagnostics.iter().any(|d| d.contains("missing") && d.contains("a.md")),
        "unexpected diagnostics: {diagnostics:?}"
    );
}

/// A link's diagnostic answers a question about another file, so it goes stale
/// when that file changes. Editing the heading a link points at must update the
/// diagnostics of every open document linking to it, with no edit of their own.
#[tokio::test]
async fn test_dependent_document_is_relinted_when_its_target_changes() {
    let temp = tempfile::tempdir().unwrap();
    let root = write_workspace(
        &temp,
        &[
            (".rumdl.toml", ENABLE_MD051),
            ("a.md", "# Heading\n"),
            ("b.md", "# B\n\nSee [target](./a.md#heading).\n"),
        ],
    );
    let (a, b) = (root.join("a.md"), root.join("b.md"));

    let client = LspTestClient::start(&root.join(".rumdl.toml"));
    client.initialize(&root, push_capabilities()).await;
    client.notify("initialized", json!({})).await;
    client.wait_for_index_ready().await;

    client.did_open(&b, "# B\n\nSee [target](./a.md#heading).\n").await;
    let first = client.wait_for_publishes(&b, 1).await;
    assert!(
        !first[0].iter().any(|d| d.contains("Link fragment")),
        "control: the link resolves before the rename, got {first:?}"
    );

    client.did_open(&a, "# Heading\n").await;
    client.did_change(&a, 2, "# Renamed\n").await;

    let diagnostics = client.wait_for_diagnostic(&b, "Link fragment").await;
    assert!(
        diagnostics.iter().any(|d| d.contains("heading") && d.contains("a.md")),
        "unexpected diagnostics: {diagnostics:?}"
    );
}

/// A document's own cross-file diagnostics are computed from the index entry it
/// had before the keystroke, so typing a link produced an answer about the text
/// as it was one edit ago. This is the most direct form of the reported
/// symptom: the diagnostic appears only on the next edit.
#[tokio::test]
async fn test_document_is_relinted_for_a_link_it_just_gained() {
    let temp = tempfile::tempdir().unwrap();
    let root = write_workspace(
        &temp,
        &[
            (".rumdl.toml", ENABLE_MD051),
            ("a.md", "# Heading\n"),
            ("b.md", "# B\n\nNothing here yet.\n"),
        ],
    );
    let b = root.join("b.md");

    let client = LspTestClient::start(&root.join(".rumdl.toml"));
    client.initialize(&root, push_capabilities()).await;
    client.notify("initialized", json!({})).await;
    client.wait_for_index_ready().await;

    client.did_open(&b, "# B\n\nNothing here yet.\n").await;
    let first = client.wait_for_publishes(&b, 1).await;
    assert!(
        !first[0].iter().any(|d| d.contains("Link fragment")),
        "control: the document holds no link before the edit, got {first:?}"
    );

    client.did_change(&b, 2, "# B\n\nSee [target](./a.md#missing).\n").await;

    let diagnostics = client.wait_for_diagnostic(&b, "Link fragment").await;
    assert!(
        diagnostics.iter().any(|d| d.contains("missing") && d.contains("a.md")),
        "unexpected diagnostics: {diagnostics:?}"
    );
}

/// A pull client is never sent diagnostics, so republishing says nothing to it.
/// The spec's mechanism for "the answers you hold may have changed" is a
/// `workspace/diagnostic/refresh` request, which is the only way an editor like
/// VS Code learns to ask again after the index finishes building.
#[tokio::test]
async fn test_pull_client_is_asked_to_refresh_when_the_index_becomes_ready() {
    let temp = tempfile::tempdir().unwrap();
    let root = write_workspace(
        &temp,
        &[
            (".rumdl.toml", ENABLE_MD051),
            ("a.md", "# Heading\n"),
            ("b.md", "# B\n\nSee [target](./a.md#missing).\n"),
        ],
    );
    let b = root.join("b.md");

    let client = LspTestClient::start(&root.join(".rumdl.toml"));
    client.initialize(&root, pull_capabilities()).await;
    client.did_open(&b, "# B\n\nSee [target](./a.md#missing).\n").await;

    // A pull client's documents are answered with an empty publish, which is
    // also the signal that the open has been processed and so precedes the
    // rescan the next notification triggers.
    client.wait_for_publishes(&b, 1).await;
    client.notify("initialized", json!({})).await;

    client
        .wait_for("a workspace/diagnostic/refresh request", |messages| {
            messages
                .iter()
                .find(|m| m.get("method").and_then(Value::as_str) == Some("workspace/diagnostic/refresh"))
                .cloned()
        })
        .await;
}

/// Diagnostics belong to documents the editor has open. Files read from disk to
/// answer a request are cached beside the open ones, so a refresh that does not
/// tell the two apart puts diagnostics on screen for a file the user never
/// opened, and nothing clears them: there is no `didClose` for a document that
/// was never opened.
///
/// Hovering a link is the way an editor reaches this without the user doing
/// anything unusual, since the preview reads the link's target.
#[tokio::test]
async fn test_a_configuration_change_does_not_publish_for_a_file_read_from_disk() {
    let temp = tempfile::tempdir().unwrap();
    let root = write_workspace(
        &temp,
        &[
            (".rumdl.toml", ENABLE_MD051),
            ("a.md", "# A\n\nSee [target](./b.md#missing).\n"),
            ("b.md", "# B\n\nSee [other](./a.md#nope).\n"),
        ],
    );
    let (a, b) = (root.join("a.md"), root.join("b.md"));

    let client = LspTestClient::start(&root.join(".rumdl.toml"));
    client.initialize(&root, push_capabilities()).await;
    client.notify("initialized", json!({})).await;
    client.wait_for_index_ready().await;

    client.did_open(&a, "# A\n\nSee [target](./b.md#missing).\n").await;
    client.wait_for_publishes(&a, 1).await;

    // Hovering the link previews b.md, which reads and caches it. The cursor
    // sits inside the destination, which is where a link hover is answered.
    let hover = client
        .request(
            "textDocument/hover",
            json!({
                "textDocument": {"uri": Url::from_file_path(&a).unwrap()},
                "position": {"line": 2, "character": 16},
            }),
        )
        .await;
    assert!(
        hover.get("result").is_some_and(|result| !result.is_null()),
        "control: the hover must produce a preview, or b.md was never cached and \
         this test cannot observe the defect, got {hover}"
    );

    client
        .notify("workspace/didChangeConfiguration", json!({"settings": {}}))
        .await;

    // The refresh reaching a.md is the control: it proves the configuration
    // change did republish, so b.md's silence is the filter and not a no-op.
    client.wait_for_publishes(&a, 2).await;
    tokio::time::sleep(Duration::from_millis(500)).await;

    let published_for_b = {
        let messages = client.received.lock().await;
        let uri = Url::from_file_path(&b).unwrap().to_string();
        messages
            .iter()
            .filter(|m| m.get("method").and_then(Value::as_str) == Some("textDocument/publishDiagnostics"))
            .filter(|m| publish_uri(m).as_deref() == Some(uri.as_str()))
            .map(publish_messages)
            .collect::<Vec<_>>()
    };
    assert!(
        published_for_b.is_empty(),
        "b.md was never opened, so nothing may publish diagnostics for it, got {published_for_b:?}"
    );
}

/// The server's background tasks must not outlive the connection they serve.
///
/// Both of them hold what the other waits on: the index worker holds the
/// re-lint sender, and the re-lint task holds a server carrying the index
/// worker's sender. If either holds its side strongly the pair keeps itself,
/// and the whole server state, alive for the life of the process after the
/// editor is gone. A client that closes its connection without sending
/// `shutdown` is the case that reaches it.
#[tokio::test(flavor = "multi_thread")]
async fn test_dropping_the_server_stops_its_background_tasks() {
    let metrics = tokio::runtime::Handle::current().metrics();
    let before = metrics.num_alive_tasks();

    {
        let (service, _socket) = LspService::new(|client| RumdlLanguageServer::new(client, None));
        // Dropped without a `shutdown` request, as a disconnecting client does.
        drop(service);
    }

    // The worker notices on its next poll, so this is a wait rather than an
    // immediate assertion.
    let deadline = Instant::now() + WAIT_TIMEOUT;
    while metrics.num_alive_tasks() > before && Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    assert_eq!(
        metrics.num_alive_tasks(),
        before,
        "background tasks outlived the server they belong to"
    );
}

/// A deletion is handled immediately and a change is debounced, so an edit made
/// shortly before a file is deleted is still queued when the deletion arrives.
/// Flushing it afterwards puts the file back in the index, and the cross-file
/// rules then keep answering questions about a file that is no longer there.
///
/// Renaming reaches this on every keystroke-then-rename: the editor reports a
/// rename as a deletion of the old path.
#[tokio::test]
async fn test_a_deleted_file_is_not_resurrected_by_a_pending_edit() {
    let temp = tempfile::tempdir().unwrap();
    let root = write_workspace(
        &temp,
        &[
            (".rumdl.toml", ENABLE_MD051),
            (
                "a.md",
                "# A\n\nSee [gone](./b.md#missing) and [kept](./c.md#missing).\n",
            ),
            ("b.md", "# B\n"),
            ("c.md", "# C\n"),
        ],
    );
    let (a, b) = (root.join("a.md"), root.join("b.md"));
    let a_text = "# A\n\nSee [gone](./b.md#missing) and [kept](./c.md#missing).\n";

    let client = LspTestClient::start(&root.join(".rumdl.toml"));
    client.initialize(&root, push_capabilities()).await;
    client.notify("initialized", json!({})).await;
    client.wait_for_index_ready().await;

    client.did_open(&a, a_text).await;
    let before = client.wait_for_publishes(&a, 1).await;
    assert!(
        before[0].iter().any(|d| d.contains("./b.md")),
        "control: b.md is indexed and lacks the anchor, so its fragment is reported \
         before the deletion, got {before:?}"
    );

    // The edit queues a debounced update for b.md; the deletion arrives while
    // that update is still waiting.
    client.did_open(&b, "# B\n").await;
    client.did_change(&b, 2, "# B\n\nEdited.\n").await;
    std::fs::remove_file(&b).unwrap();
    client
        .notify(
            "workspace/didChangeWatchedFiles",
            json!({"changes": [{"uri": Url::from_file_path(&b).unwrap(), "type": 3}]}),
        )
        .await;

    // Long enough for the pending edit to have been flushed if it survived the
    // deletion, which is the whole failure mode.
    tokio::time::sleep(Duration::from_millis(600)).await;
    let resurrected = {
        let index = client.server.workspace_index.read().await;
        index.get_file(&b).is_some()
    };
    assert!(!resurrected, "a deleted file must not be back in the workspace index");

    // What the editor shows: an edit of a.md re-lints it, and the link into the
    // deleted file no longer has an index entry to be judged against. Count the
    // publishes first, so this reads the lint that ran after the deletion rather
    // than the one from did_open.
    let seen = client.wait_for_publishes(&a, 1).await.len();
    let a_edited = "# A\n\nSee [gone](./b.md#missing) and [kept](./c.md#missing). Edited.\n";
    client.did_change(&a, 2, a_edited).await;
    let after = client.wait_for_publishes(&a, seen + 1).await;
    let latest = after.last().unwrap();
    assert!(
        latest.iter().any(|d| d.contains("./c.md")),
        "control: the surviving file is still judged, so a lint that simply went \
         quiet cannot pass for a fixed one, got {latest:?}"
    );
    assert!(
        !latest.iter().any(|d| d.contains("./b.md")),
        "the deleted file must stop answering fragment questions, got {latest:?}"
    );
}
