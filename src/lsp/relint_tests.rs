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

/// A file an editor has open is not the filesystem's to take out of the index:
/// `did_open`/`did_change` index it whatever discovery says (`server.rs`), so a
/// watch event naming it is answered from the buffer rather than becoming an
/// eviction. Both ways an editor speaks for a document have to survive, so one
/// file here is opened and edited and the other only opened.
///
/// Their updates are still waiting out the debounce window when the event
/// arrives, and an eviction discards those along with the entry.
#[tokio::test]
async fn test_a_watcher_event_for_an_open_file_keeps_the_edit_that_was_still_waiting() {
    let temp = tempfile::tempdir().unwrap();
    let root = write_workspace(
        &temp,
        &[
            (
                ".rumdl.toml",
                "[global]\nenable = [\"MD051\"]\nexclude = [\"skipped.md\", \"opened.md\"]\n",
            ),
            ("skipped.md", "# Skipped\n"),
            ("opened.md", "# Opened\n"),
        ],
    );
    let skipped = root.join("skipped.md");
    let opened = root.join("opened.md");

    let client = LspTestClient::start(&root.join(".rumdl.toml"));
    client.initialize(&root, push_capabilities()).await;
    client.notify("initialized", json!({})).await;
    client.wait_for_index_ready().await;

    // Opening an excluded file indexes it even though discovery skips it.
    client.did_open(&skipped, "# Skipped\n").await;
    client.did_change(&skipped, 2, "# Skipped\n\nEdited.\n").await;
    client.did_open(&opened, "# Opened\n").await;

    // The watcher reports the saves while those updates are still debounced. The
    // files are excluded, so discovery is what would answer for them.
    client
        .notify(
            "workspace/didChangeWatchedFiles",
            json!({"changes": [
                {"uri": Url::from_file_path(&skipped).unwrap(), "type": 2},
                {"uri": Url::from_file_path(&opened).unwrap(), "type": 2},
            ]}),
        )
        .await;

    tokio::time::sleep(Duration::from_millis(500)).await;
    let (edited_indexed, opened_indexed) = {
        let index = client.server.workspace_index.read().await;
        (index.get_file(&skipped).is_some(), index.get_file(&opened).is_some())
    };
    assert!(
        edited_indexed,
        "an open file that discovery skips is still indexed through its own edits"
    );
    assert!(
        opened_indexed,
        "opening it is the editor speaking for it just as much as editing it is"
    );
}

/// The other half of that pair: an update the watcher read from disk is nobody
/// asking for the file, so an eviction has to throw it away.
///
/// Adding a pattern to `.gitignore` reloads no configuration and triggers no
/// rescan, so the eviction is the only thing that ever speaks for the file. A
/// disk read flushed after it puts a file the index no longer covers back in,
/// for good.
#[tokio::test]
async fn test_an_evicted_file_drops_the_disk_read_that_was_still_waiting() {
    let temp = tempfile::tempdir().unwrap();
    let root = write_workspace(
        &temp,
        &[
            (".rumdl.toml", ENABLE_MD051),
            ("generated.md", "# Generated\n"),
            ("decoy.md", "# Decoy\n"),
        ],
    );
    let generated = root.join("generated.md");
    let decoy = root.join("decoy.md");

    let client = LspTestClient::start(&root.join(".rumdl.toml"));
    client.initialize(&root, push_capabilities()).await;
    client.notify("initialized", json!({})).await;
    client.wait_for_index_ready().await;

    // A save no editor made: the watcher reads the file from disk while it is
    // still one discovery covers. The deletion riding along behind it is a
    // barrier, not part of the scenario: a change is debounced and a deletion is
    // not, so the decoy leaving the index proves the server got this far and
    // therefore that the read above is queued.
    std::fs::write(&generated, "# Generated\n\n## Added Anchor\n").unwrap();
    std::fs::remove_file(&decoy).unwrap();
    client
        .notify(
            "workspace/didChangeWatchedFiles",
            json!({"changes": [
                {"uri": Url::from_file_path(&generated).unwrap(), "type": 2},
                {"uri": Url::from_file_path(&decoy).unwrap(), "type": 3},
            ]}),
        )
        .await;

    // Both halves of what the eviction below has to contend with, proven rather
    // than assumed: the read is queued, and it is still waiting out its debounce
    // window. Without this the eviction could be removing an already-flushed
    // entry, or a file the server never read at all, and the assertion at the
    // end would pass having exercised nothing.
    let deadline = Instant::now() + WAIT_TIMEOUT;
    loop {
        let index = client.server.workspace_index.read().await;
        if index.get_file(&decoy).is_none() {
            assert_eq!(
                index.get_file(&generated).map(|file| file.headings.len()),
                Some(1),
                "the debounce window closed before the eviction, so this run proves nothing"
            );
            break;
        }
        drop(index);
        assert!(Instant::now() < deadline, "the watch notification was never processed");
        tokio::time::sleep(Duration::from_millis(1)).await;
    }

    // Now an ignore rule starts matching the file, and the next watch event
    // arrives as an eviction rather than an update.
    std::fs::write(root.join(".gitignore"), "generated.md\n").unwrap();
    client
        .notify(
            "workspace/didChangeWatchedFiles",
            json!({"changes": [{"uri": Url::from_file_path(&generated).unwrap(), "type": 2}]}),
        )
        .await;

    // Long enough for the waiting read to have been flushed if it survived.
    tokio::time::sleep(Duration::from_millis(500)).await;
    let resurrected = {
        let index = client.server.workspace_index.read().await;
        index.get_file(&generated).is_some()
    };
    assert!(
        !resurrected,
        "a file the index stopped covering must not be put back by a disk read that was already waiting"
    );
}

/// Anchors the index holds for `path`, for asserting what an edit reached.
async fn indexed_anchors(client: &LspTestClient, path: &Path) -> Vec<String> {
    let index = client.server.workspace_index.read().await;
    index
        .get_file(path)
        .map(|file| {
            file.headings
                .iter()
                .map(|heading| heading.auto_anchor.clone())
                .collect()
        })
        .unwrap_or_default()
}

/// A rescan re-reads the workspace from disk, so an edit still waiting out its
/// debounce window has to outlive it: dropping the edit leaves the index holding
/// the saved copy of a file the editor has since changed.
#[tokio::test]
async fn test_a_rescan_keeps_the_edit_that_was_still_waiting() {
    let temp = tempfile::tempdir().unwrap();
    let root = write_workspace(&temp, &[(".rumdl.toml", ENABLE_MD051), ("a.md", "# A\n")]);
    let a = root.join("a.md");

    let client = LspTestClient::start(&root.join(".rumdl.toml"));
    client.initialize(&root, push_capabilities()).await;
    client.notify("initialized", json!({})).await;
    client.wait_for_index_ready().await;

    client.did_open(&a, "# A\n").await;
    client.did_change(&a, 2, "# A\n\n## Live Anchor\n").await;
    // The publish for that edit is the barrier: the handler queues the index
    // update before it lints, so the edit is with the worker by the time this
    // returns, and asserting the index has yet to see it proves it is still
    // waiting rather than already applied.
    client.wait_for_publishes(&a, 2).await;
    assert_eq!(
        indexed_anchors(&client, &a).await,
        vec!["a"],
        "the debounce window closed before the rescan, so this run proves nothing"
    );

    // A config change rebuilds the index from what is on disk, where the new
    // heading has never been written.
    std::fs::write(root.join(".rumdl.toml"), "[global]\nenable = [\"MD051\", \"MD047\"]\n").unwrap();
    client
        .notify(
            "workspace/didChangeWatchedFiles",
            json!({"changes": [{"uri": Url::from_file_path(root.join(".rumdl.toml")).unwrap(), "type": 2}]}),
        )
        .await;

    tokio::time::sleep(Duration::from_millis(500)).await;
    assert_eq!(
        indexed_anchors(&client, &a).await,
        vec!["a", "live-anchor"],
        "the rescan must not cost the editor an edit that was already on its way to the index"
    );
}

/// The same requirement once the edit has landed: a rescan reads every file from
/// disk, so an open document with unsaved changes must be indexed from the
/// buffer the editor holds rather than the copy the filesystem still has.
#[tokio::test]
async fn test_a_rescan_indexes_an_open_buffer_rather_than_its_saved_copy() {
    let temp = tempfile::tempdir().unwrap();
    let root = write_workspace(&temp, &[(".rumdl.toml", ENABLE_MD051), ("a.md", "# A\n")]);
    let a = root.join("a.md");

    let client = LspTestClient::start(&root.join(".rumdl.toml"));
    client.initialize(&root, push_capabilities()).await;
    client.notify("initialized", json!({})).await;
    client.wait_for_index_ready().await;

    client.did_open(&a, "# A\n").await;
    client.did_change(&a, 2, "# A\n\n## Live Anchor\n").await;

    // Unlike the test above, this one waits for the edit to reach the index, so
    // what the rescan meets is a document whose buffer and saved copy differ.
    let deadline = Instant::now() + WAIT_TIMEOUT;
    while indexed_anchors(&client, &a).await != vec!["a", "live-anchor"] {
        assert!(Instant::now() < deadline, "the edit never reached the index");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    std::fs::write(root.join(".rumdl.toml"), "[global]\nenable = [\"MD051\", \"MD047\"]\n").unwrap();
    client
        .notify(
            "workspace/didChangeWatchedFiles",
            json!({"changes": [{"uri": Url::from_file_path(root.join(".rumdl.toml")).unwrap(), "type": 2}]}),
        )
        .await;

    tokio::time::sleep(Duration::from_millis(500)).await;
    assert_eq!(
        indexed_anchors(&client, &a).await,
        vec!["a", "live-anchor"],
        "a rescan must not revert an open document to what was last saved"
    );
}

/// A watcher event is the filesystem speaking about a file the editor may be
/// holding unsaved changes to. The saved copy is what the user stopped looking
/// at, so it must not replace the buffer in the index.
#[tokio::test]
async fn test_a_watcher_event_does_not_replace_an_open_buffer_with_its_saved_copy() {
    let temp = tempfile::tempdir().unwrap();
    let root = write_workspace(&temp, &[(".rumdl.toml", ENABLE_MD051), ("a.md", "# A\n")]);
    let a = root.join("a.md");

    let client = LspTestClient::start(&root.join(".rumdl.toml"));
    client.initialize(&root, push_capabilities()).await;
    client.notify("initialized", json!({})).await;
    client.wait_for_index_ready().await;

    client.did_open(&a, "# A\n").await;
    client.did_change(&a, 2, "# A\n\n## Live Anchor\n").await;

    // Waiting for the edit to land is what makes the watcher event meet a
    // document whose buffer and saved copy differ.
    let deadline = Instant::now() + WAIT_TIMEOUT;
    while indexed_anchors(&client, &a).await != vec!["a", "live-anchor"] {
        assert!(Instant::now() < deadline, "the edit never reached the index");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    // Anything that touches the file reports here: a save, a branch switch, a
    // formatter. The heading was never written, so disk still holds `# A`.
    client
        .notify(
            "workspace/didChangeWatchedFiles",
            json!({"changes": [{"uri": Url::from_file_path(&a).unwrap(), "type": 2}]}),
        )
        .await;

    tokio::time::sleep(Duration::from_millis(500)).await;
    assert_eq!(
        indexed_anchors(&client, &a).await,
        vec!["a", "live-anchor"],
        "a document the editor has open is answered for by its buffer, not by the filesystem"
    );
}

/// A watch event is the filesystem's wording of a path while the document store
/// is keyed by the editor's, and under a symlinked root those are two spellings
/// of one file. Compared literally the buffer goes unrecognized, and the saved
/// copy replaces it exactly as if the document were closed.
#[cfg(unix)]
#[tokio::test]
async fn test_a_watcher_event_finds_the_open_buffer_under_another_spelling() {
    let temp = tempfile::tempdir().unwrap();
    let base = crate::lsp::resolve_workspace_root(temp.path());
    let root = base.join("real");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join(".rumdl.toml"), ENABLE_MD051).unwrap();
    std::fs::write(root.join("a.md"), "# A\n").unwrap();
    let link = base.join("link");
    std::os::unix::fs::symlink(&root, &link).unwrap();
    let a = root.join("a.md");

    let client = LspTestClient::start(&root.join(".rumdl.toml"));
    client.initialize(&root, push_capabilities()).await;
    client.notify("initialized", json!({})).await;
    client.wait_for_index_ready().await;

    client.did_open(&a, "# A\n").await;
    client.did_change(&a, 2, "# A\n\n## Live Anchor\n").await;
    let deadline = Instant::now() + WAIT_TIMEOUT;
    while indexed_anchors(&client, &a).await != vec!["a", "live-anchor"] {
        assert!(Instant::now() < deadline, "the edit never reached the index");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    // The same file, named through the symlink the editor never used.
    client
        .notify(
            "workspace/didChangeWatchedFiles",
            json!({"changes": [{"uri": Url::from_file_path(link.join("a.md")).unwrap(), "type": 2}]}),
        )
        .await;

    tokio::time::sleep(Duration::from_millis(500)).await;
    assert_eq!(
        indexed_anchors(&client, &a).await,
        vec!["a", "live-anchor"],
        "a spelling of the path the editor did not use still names a document it has open"
    );
}

/// Opening a file indexes it whether or not discovery would find it, so a rescan
/// of the workspace must not be the thing that takes it back out: the editor
/// still has it open, and nothing speaks for it again until the user types.
#[tokio::test]
async fn test_a_rescan_keeps_a_file_the_editor_opened_that_discovery_skips() {
    let temp = tempfile::tempdir().unwrap();
    let root = write_workspace(
        &temp,
        &[
            (
                ".rumdl.toml",
                "[global]\nenable = [\"MD051\"]\nexclude = [\"opened.md\"]\n",
            ),
            ("opened.md", "# Opened\n"),
            ("found.md", "# Found\n"),
        ],
    );
    let opened = root.join("opened.md");

    let client = LspTestClient::start(&root.join(".rumdl.toml"));
    client.initialize(&root, push_capabilities()).await;
    client.notify("initialized", json!({})).await;
    client.wait_for_index_ready().await;

    client.did_open(&opened, "# Opened\n").await;
    let deadline = Instant::now() + WAIT_TIMEOUT;
    while indexed_anchors(&client, &opened).await != vec!["opened"] {
        assert!(Instant::now() < deadline, "opening the file never indexed it");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    // The exclude is unchanged, so discovery still skips the file. Only the
    // rescan is new.
    std::fs::write(
        root.join(".rumdl.toml"),
        "[global]\nenable = [\"MD051\", \"MD047\"]\nexclude = [\"opened.md\"]\n",
    )
    .unwrap();
    client
        .notify(
            "workspace/didChangeWatchedFiles",
            json!({"changes": [{"uri": Url::from_file_path(root.join(".rumdl.toml")).unwrap(), "type": 2}]}),
        )
        .await;

    tokio::time::sleep(Duration::from_millis(500)).await;
    assert_eq!(
        indexed_anchors(&client, &opened).await,
        vec!["opened"],
        "a rescan must not evict a document the editor still has open"
    );
}

/// Open `b.md`, wait for the index to hold it, delete it, let `replace` put
/// whatever comes next at that path, and rescan while the document is still
/// open. Answers whether the rescan put the path back in the index.
async fn rescan_after_deleting_an_open_file(replace: impl FnOnce(&Path)) -> bool {
    let temp = tempfile::tempdir().unwrap();
    let root = write_workspace(
        &temp,
        &[(".rumdl.toml", ENABLE_MD051), ("a.md", "# A\n"), ("b.md", "# B\n")],
    );
    let b = root.join("b.md");

    let client = LspTestClient::start(&root.join(".rumdl.toml"));
    client.initialize(&root, push_capabilities()).await;
    client.notify("initialized", json!({})).await;
    client.wait_for_index_ready().await;

    client.did_open(&b, "# B\n").await;
    let deadline = Instant::now() + WAIT_TIMEOUT;
    while indexed_anchors(&client, &b).await != vec!["b"] {
        assert!(Instant::now() < deadline, "control: opening the file never indexed it");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    std::fs::remove_file(&b).unwrap();
    client
        .notify(
            "workspace/didChangeWatchedFiles",
            json!({"changes": [{"uri": Url::from_file_path(&b).unwrap(), "type": 3}]}),
        )
        .await;
    let deadline = Instant::now() + WAIT_TIMEOUT;
    while client.server.workspace_index.read().await.get_file(&b).is_some() {
        assert!(Instant::now() < deadline, "the deletion never reached the index");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    replace(&b);

    // The document is still open, so the rescan sees a buffer for a path that no
    // longer names a file.
    std::fs::write(root.join(".rumdl.toml"), "[global]\nenable = [\"MD051\", \"MD047\"]\n").unwrap();
    client
        .notify(
            "workspace/didChangeWatchedFiles",
            json!({"changes": [{"uri": Url::from_file_path(root.join(".rumdl.toml")).unwrap(), "type": 2}]}),
        )
        .await;

    tokio::time::sleep(Duration::from_millis(500)).await;
    assert!(
        client
            .server
            .workspace_index
            .read()
            .await
            .get_file(&root.join("a.md"))
            .is_some(),
        "control: the rescan ran and indexed the file that is still there"
    );
    client.server.workspace_index.read().await.get_file(&b).is_some()
}

/// Where the editor's authority stops: a path the filesystem no longer has is
/// not the editor's to keep. A rename reaches the server as a deletion of the
/// old path and the document can still be open under it, so a rescan reading the
/// buffer would put the old name back and the cross-file rules would go on
/// answering for it.
#[tokio::test]
async fn test_a_rescan_does_not_restore_a_file_deleted_while_the_editor_had_it_open() {
    assert!(
        !rescan_after_deleting_an_open_file(|_| {}).await,
        "a rescan must not index a buffer whose file has been deleted"
    );
}

/// The same requirement where the path still answers: a directory taking the
/// deleted file's name exists, so asking only whether the path is there says yes
/// for something the workspace scan would never hand back.
#[tokio::test]
async fn test_a_rescan_does_not_restore_an_open_file_a_directory_has_replaced() {
    assert!(
        !rescan_after_deleting_an_open_file(|path| std::fs::create_dir(path).unwrap()).await,
        "a rescan must not index a buffer whose path is no longer a file"
    );
}

/// The same requirement with nothing in flight to explain it: an ignore rule
/// that starts matching an open document arrives long after the document's own
/// update has landed, so keeping the entry cannot be a waiting update putting it
/// back. Adding a pattern to `.gitignore` reloads no configuration and triggers
/// no rescan, so a watch event evicting the file here is the last word.
#[tokio::test]
async fn test_an_ignore_rule_does_not_evict_a_document_the_editor_has_open() {
    let temp = tempfile::tempdir().unwrap();
    let root = write_workspace(&temp, &[(".rumdl.toml", ENABLE_MD051), ("a.md", "# A\n")]);
    let a = root.join("a.md");

    let client = LspTestClient::start(&root.join(".rumdl.toml"));
    client.initialize(&root, push_capabilities()).await;
    client.notify("initialized", json!({})).await;
    client.wait_for_index_ready().await;

    client.did_open(&a, "# A\n").await;
    let deadline = Instant::now() + WAIT_TIMEOUT;
    while indexed_anchors(&client, &a).await != vec!["a"] {
        assert!(Instant::now() < deadline, "opening the file never indexed it");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    // Well past the debounce window, so nothing is left waiting to re-add it.
    tokio::time::sleep(Duration::from_millis(300)).await;

    std::fs::write(root.join(".gitignore"), "a.md\n").unwrap();
    client
        .notify(
            "workspace/didChangeWatchedFiles",
            json!({"changes": [{"uri": Url::from_file_path(&a).unwrap(), "type": 2}]}),
        )
        .await;

    tokio::time::sleep(Duration::from_millis(500)).await;
    assert_eq!(
        indexed_anchors(&client, &a).await,
        vec!["a"],
        "an ignore rule must not take away a document the editor is showing"
    );
}
