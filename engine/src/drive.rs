use crate::{auth::Auth, store::Node};
use anyhow::{Context, Result};
use parking_lot::Mutex;
use reqwest::{
    blocking::{Client, Response},
    header,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{fs::File, io::Read, path::Path, sync::Arc, time::Duration};

const API: &str = "https://www.googleapis.com/drive/v3";
const FIELDS: &str = "id,name,mimeType,parents,size,version,modifiedTime,trashed";

pub type ChangeBatch = (Vec<(String, Option<Node>)>, String);
pub trait Remote: Send + Sync {
    fn list(&self, parent: &str) -> Result<Vec<Node>>;
    fn get(&self, id: &str) -> Result<Option<Node>>;
    fn generate_id(&self) -> Result<String>;
    fn download(&self, id: &str, target: &Path) -> Result<()>;
    fn read_range(&self, _node: &Node, _offset: u64, _length: usize) -> Result<Vec<u8>> {
        anyhow::bail!("This provider does not support streaming reads")
    }
    fn upload(&self, node: &Node, source: &Path, exists: bool) -> Result<Node> {
        self.upload_source(node, &crate::streaming::FileSource(source), exists)
    }
    fn upload_source(
        &self,
        _node: &Node,
        _source: &dyn crate::streaming::ContentSource,
        _exists: bool,
    ) -> Result<Node> {
        anyhow::bail!("This provider does not support streaming uploads")
    }
    fn mkdir(&self, node: &Node) -> Result<Node>;
    fn relocate(&self, node: &Node, old_parent: &str) -> Result<Node>;
    fn trash(&self, id: &str) -> Result<()>;
    fn cursor(&self) -> Result<String>;
    fn changes(&self, cursor: &str) -> Result<ChangeBatch>;
    fn about(&self) -> Result<Value>;
}
pub struct GoogleDrive {
    auth: Arc<Auth>,
    http: Client,
    pub progress: Arc<Mutex<Option<Value>>>,
    range_tags: Mutex<std::collections::HashMap<(String, String), String>>,
    #[cfg(test)]
    test_origin: Option<url::Url>,
}
struct Progress(Arc<Mutex<Option<Value>>>);
impl Drop for Progress {
    fn drop(&mut self) {
        *self.0.lock() = None;
    }
}
#[derive(Serialize, Deserialize)]
struct UploadSession {
    url: String,
    size: u64,
    modified: u128,
    base_version: String,
}
fn acknowledged(range: Option<&str>) -> Result<u64> {
    match range {
        None => Ok(0),
        Some(s) => {
            let (start, end) = s
                .strip_prefix("bytes=")
                .context("Invalid upload Range")?
                .split_once('-')
                .context("Invalid upload Range")?;
            anyhow::ensure!(start == "0", "Invalid upload Range start");
            Ok(end
                .parse::<u64>()?
                .checked_add(1)
                .context("Upload range overflow")?)
        }
    }
}
impl GoogleDrive {
    pub fn new(auth: Arc<Auth>) -> Result<Self> {
        Ok(Self {
            auth,
            http: Client::builder()
                .connect_timeout(Duration::from_secs(10))
                .timeout(Duration::from_secs(120))
                .build()?,
            progress: Arc::new(Mutex::new(None)),
            range_tags: Mutex::new(std::collections::HashMap::new()),
            #[cfg(test)]
            test_origin: None,
        })
    }
    fn request(
        &self,
        method: reqwest::Method,
        url: &str,
    ) -> Result<reqwest::blocking::RequestBuilder> {
        #[cfg(test)]
        let rewritten = self.test_origin.as_ref().map(|origin| {
            let original = url::Url::parse(url).expect("fixture URL");
            let mut next = origin.clone();
            next.set_path(original.path());
            next.set_query(original.query());
            next.to_string()
        });
        #[cfg(test)]
        let url = rewritten.as_deref().unwrap_or(url);
        Ok(self
            .http
            .request(method, url)
            .bearer_auth(self.auth.bearer()?))
    }
    fn checked(response: Response) -> Result<Response> {
        let status = response.status();
        anyhow::ensure!(
            status.is_success(),
            "Google Drive HTTP {}: {}",
            status.as_u16(),
            match status.as_u16() {
                401 => "Sign in again",
                403 => "Permission denied or Google quota exceeded",
                429 => "Rate limited; retrying later",
                507 => "Storage full",
                _ => "Request failed; pending work is retained",
            }
        );
        Ok(response)
    }
}
fn node(v: Value) -> Result<Node> {
    Ok(Node {
        id: v["id"].as_str().context("Missing Drive ID")?.into(),
        parent: v["parents"][0].as_str().unwrap_or("root").into(),
        name: v["name"].as_str().unwrap_or("unnamed").into(),
        alias: String::new(),
        directory: v["mimeType"] == "application/vnd.google-apps.folder",
        size: v["size"].as_str().unwrap_or("0").parse().unwrap_or(0),
        version: v["version"].as_str().unwrap_or("").into(),
        cached: false,
        dirty: false,
        deleted: v["trashed"].as_bool().unwrap_or(false),
        last_access: 0,
        modified: v["modifiedTime"]
            .as_str()
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .map(|d| d.timestamp())
            .unwrap_or(0),
    })
}
fn supported(v: &Value) -> bool {
    let mime = v["mimeType"].as_str().unwrap_or("");
    !mime.starts_with("application/vnd.google-apps.")
        || mime == "application/vnd.google-apps.folder"
}
impl Remote for GoogleDrive {
    fn list(&self, parent: &str) -> Result<Vec<Node>> {
        let mut out = vec![];
        let mut page = String::new();
        loop {
            let q = format!(
                "'{}' in parents and trashed = false",
                parent.replace('\\', "\\\\").replace('\'', "\\'")
            );
            let mut request = self
                .request(reqwest::Method::GET, &format!("{API}/files"))?
                .query(&[
                    ("q", q.as_str()),
                    ("spaces", "drive"),
                    ("pageSize", "1000"),
                    ("fields", &format!("nextPageToken,files({FIELDS})")),
                ]);
            if !page.is_empty() {
                request = request.query(&[("pageToken", page.as_str())]);
            }
            let v: Value = Self::checked(request.send()?)?.json()?;
            for f in v["files"].as_array().context("Invalid file listing")? {
                if supported(f) {
                    out.push(node(f.clone())?);
                }
            }
            match v["nextPageToken"].as_str() {
                Some(s) => page = s.into(),
                None => break,
            }
        }
        Ok(out)
    }
    fn get(&self, id: &str) -> Result<Option<Node>> {
        let r = self
            .request(reqwest::Method::GET, &format!("{API}/files/{id}"))?
            .timeout(Duration::from_secs(15))
            .query(&[("fields", FIELDS)])
            .send()?;
        if r.status().as_u16() == 404 {
            return Ok(None);
        }
        let v: Value = Self::checked(r)?.json()?;
        if !supported(&v) || v["trashed"] == true {
            return Ok(None);
        }
        Ok(Some(node(v)?))
    }
    fn generate_id(&self) -> Result<String> {
        let v: Value = Self::checked(
            self.request(reqwest::Method::GET, &format!("{API}/files/generateIds"))?
                .query(&[("count", "1"), ("space", "drive")])
                .send()?,
        )?
        .json()?;
        Ok(v["ids"][0].as_str().context("No generated ID")?.into())
    }
    fn download(&self, id: &str, target: &Path) -> Result<()> {
        let _guard = Progress(self.progress.clone());
        let mut response = Self::checked(
            self.request(reqwest::Method::GET, &format!("{API}/files/{id}"))?
                .query(&[("alt", "media")])
                .send()?,
        )?;
        let total = response.content_length().unwrap_or(0);
        let mut count = 0;
        let mut file = File::create(target)?;
        let mut bytes = vec![0u8; 1024 * 1024];
        loop {
            let length = response.read(&mut bytes)?;
            if length == 0 {
                break;
            }
            std::io::Write::write_all(&mut file, &bytes[..length])?;
            count += length as u64;
            *self.progress.lock() =
                Some(json!({"direction":"Downloading","name":id,"bytes":count,"total":total}));
        }
        file.sync_all()?;
        Ok(())
    }
    fn read_range(&self, n: &Node, offset: u64, length: usize) -> Result<Vec<u8>> {
        if length == 0 || offset >= n.size {
            return Ok(vec![]);
        }
        anyhow::ensure!(
            length <= crate::streaming::BLOCK,
            "Range request exceeds block limit"
        );
        let length = (n.size - offset).min(length as u64) as usize;
        let end = offset + length as u64 - 1;
        let key = (n.id.clone(), n.version.clone());
        let tag = self.range_tags.lock().get(&key).cloned();
        let mut request = self
            .request(reqwest::Method::GET, &format!("{API}/files/{}", n.id))?
            .query(&[("alt", "media")])
            .header(header::RANGE, format!("bytes={offset}-{end}"))
            .timeout(Duration::from_secs(20));
        if let Some(tag) = &tag {
            request = request.header(header::IF_MATCH, tag);
        }
        let response = request.send()?;
        anyhow::ensure!(
            response.status().as_u16() != 412,
            "Remote file changed; close and reopen it"
        );
        let response = Self::checked(response)?;
        let expected = format!("bytes {offset}-{end}/{}", n.size);
        anyhow::ensure!(
            response.status().as_u16() == 206
                && response
                    .headers()
                    .get(header::CONTENT_RANGE)
                    .and_then(|v| v.to_str().ok())
                    == Some(expected.as_str())
                || response.status().as_u16() == 200 && offset == 0 && length as u64 == n.size,
            "Google ignored the requested range; full download was refused"
        );
        let next_tag = response
            .headers()
            .get(header::ETAG)
            .and_then(|v| v.to_str().ok())
            .filter(|tag| !tag.starts_with("W/"))
            .map(str::to_owned);
        if let Some(tag) = &tag {
            anyhow::ensure!(
                next_tag.as_ref() == Some(tag),
                "Remote file changed during read"
            );
        }
        let mut bytes = Vec::with_capacity(length);
        response.take(length as u64 + 1).read_to_end(&mut bytes)?;
        anyhow::ensure!(bytes.len() == length, "Incomplete ranged response");
        if tag.is_none() {
            let current = self.get(&n.id)?.context("Remote file was removed")?;
            anyhow::ensure!(
                current.version == n.version,
                "Remote file changed; close and reopen it"
            );
            if let Some(tag) = next_tag {
                let mut tags = self.range_tags.lock();
                if tags.len() >= 256 {
                    tags.clear();
                }
                tags.insert(key, tag);
            }
        }
        Ok(bytes)
    }
    fn upload_source(
        &self,
        n: &Node,
        source: &dyn crate::streaming::ContentSource,
        exists: bool,
    ) -> Result<Node> {
        if n.size == 0 {
            let request = if exists {
                self.request(
                    reqwest::Method::PATCH,
                    &format!("https://www.googleapis.com/upload/drive/v3/files/{}", n.id),
                )?
                .query(&[("uploadType", "media"), ("fields", FIELDS)])
                .header(header::CONTENT_TYPE, "application/octet-stream")
                .body(Vec::<u8>::new())
            } else {
                self.request(reqwest::Method::POST,&format!("{API}/files"))?.query(&[("fields",FIELDS)]).json(&json!({"id":n.id,"name":n.name,"parents":[n.parent],"mimeType":"application/octet-stream"}))
            };
            return node(Self::checked(request.send()?)?.json()?);
        }
        let _guard = Progress(self.progress.clone());
        *self.progress.lock() =
            Some(json!({"direction":"Uploading","name":n.alias,"bytes":0,"total":n.size}));
        let sessions = self.auth.storage_path().join("uploads");
        std::fs::create_dir_all(&sessions)?;
        use sha2::{Digest, Sha256};
        let session_path = sessions.join(format!("{:x}.dpapi", Sha256::digest(n.id.as_bytes())));
        let modified = source.fingerprint()?;
        let saved = crate::auth::load_secret(&session_path)
            .ok()
            .and_then(|s| serde_json::from_slice::<UploadSession>(&s).ok())
            .filter(|s| s.size == n.size && s.modified == modified && s.base_version == n.version);
        let mut offset = 0;
        let mut resumed = None;
        if let Some(saved) = saved {
            let parsed = url::Url::parse(&saved.url)?;
            anyhow::ensure!(
                parsed.scheme() == "https" && parsed.host_str() == Some("www.googleapis.com"),
                "Unexpected saved upload origin"
            );
            let r = self
                .request(reqwest::Method::PUT, &saved.url)?
                .header(header::CONTENT_LENGTH, 0)
                .header(header::CONTENT_RANGE, format!("bytes */{}", n.size))
                .send()?;
            if r.status().is_success() {
                let result = node(r.json()?)?;
                let _ = std::fs::remove_file(&session_path);
                return Ok(result);
            }
            if r.status().as_u16() == 308 {
                offset =
                    acknowledged(r.headers().get(header::RANGE).and_then(|s| s.to_str().ok()))?;
                resumed = Some(saved.url);
            } else if r.status().as_u16() != 404 && r.status().as_u16() != 410 {
                Self::checked(r)?;
            }
        }
        let url = if exists {
            format!("https://www.googleapis.com/upload/drive/v3/files/{}", n.id)
        } else {
            "https://www.googleapis.com/upload/drive/v3/files".into()
        };
        let metadata = if exists {
            json!({})
        } else {
            json!({"id":n.id,"name":n.name,"parents":[n.parent]})
        };
        let session = if let Some(url) = resumed {
            url
        } else {
            let r = Self::checked(
                self.request(
                    if exists {
                        reqwest::Method::PATCH
                    } else {
                        reqwest::Method::POST
                    },
                    &url,
                )?
                .query(&[("uploadType", "resumable"), ("fields", FIELDS)])
                .header("X-Upload-Content-Length", n.size)
                .header("X-Upload-Content-Type", "application/octet-stream")
                .json(&metadata)
                .send()?,
            )?;
            r.headers()
                .get(header::LOCATION)
                .context("No upload session returned")?
                .to_str()?
                .to_owned()
        };
        // Do not send the bearer token to an arbitrary redirect/session origin.
        let parsed = url::Url::parse(&session)?;
        anyhow::ensure!(
            parsed.scheme() == "https" && parsed.host_str() == Some("www.googleapis.com"),
            "Unexpected upload session origin"
        );
        crate::auth::save_secret(
            &session_path,
            &serde_json::to_vec(&UploadSession {
                url: session.clone(),
                size: n.size,
                modified,
                base_version: n.version.clone(),
            })?,
        )?;
        anyhow::ensure!(offset <= n.size, "Upload offset exceeds file size");
        loop {
            let length = (n.size - offset).min(8 * 1024 * 1024) as usize;
            let mut chunk = vec![0; length];
            source.read_exact_at(offset, &mut chunk)?;
            let range = if n.size == 0 {
                "bytes */0".into()
            } else {
                format!("bytes {}-{}/{}", offset, offset + length as u64 - 1, n.size)
            };
            let r = self
                .request(reqwest::Method::PUT, &session)?
                .header(header::CONTENT_RANGE, range)
                .header(header::CONTENT_LENGTH, length)
                .body(chunk)
                .send()?;
            if r.status().as_u16() == 308 {
                let next =
                    acknowledged(r.headers().get(header::RANGE).and_then(|s| s.to_str().ok()))?;
                anyhow::ensure!(
                    next > offset && next <= offset + length as u64 && next < n.size,
                    "Invalid resumable upload acknowledgement"
                );
                offset = next;
                *self.progress.lock() = Some(
                    json!({"direction":"Uploading","name":n.alias,"bytes":offset,"total":n.size}),
                );
                continue;
            }
            let uploaded = node(Self::checked(r)?.json()?)?;
            let _ = std::fs::remove_file(&session_path);
            return Ok(uploaded);
        }
    }
    fn mkdir(&self, n: &Node) -> Result<Node> {
        node(Self::checked(self.request(reqwest::Method::POST,&format!("{API}/files"))?.query(&[("fields",FIELDS)]).json(&json!({"id":n.id,"name":n.name,"parents":[n.parent],"mimeType":"application/vnd.google-apps.folder"})).send()?)?.json()?)
    }
    fn relocate(&self, n: &Node, old: &str) -> Result<Node> {
        let mut r = self
            .request(reqwest::Method::PATCH, &format!("{API}/files/{}", n.id))?
            .query(&[("fields", FIELDS)]);
        if old != n.parent {
            r = r.query(&[("addParents", n.parent.as_str()), ("removeParents", old)]);
        }
        node(Self::checked(r.json(&json!({"name":n.name})).send()?)?.json()?)
    }
    fn trash(&self, id: &str) -> Result<()> {
        Self::checked(
            self.request(reqwest::Method::PATCH, &format!("{API}/files/{id}"))?
                .json(&json!({"trashed":true}))
                .send()?,
        )?;
        Ok(())
    }
    fn cursor(&self) -> Result<String> {
        let v: Value = Self::checked(
            self.request(
                reqwest::Method::GET,
                &format!("{API}/changes/startPageToken"),
            )?
            .send()?,
        )?
        .json()?;
        Ok(v["startPageToken"]
            .as_str()
            .context("Missing change cursor")?
            .into())
    }
    fn changes(&self, cursor: &str) -> Result<ChangeBatch> {
        let mut page = cursor.to_string();
        let mut out = vec![];
        loop {
            let v:Value=Self::checked(self.request(reqwest::Method::GET,&format!("{API}/changes"))?.query(&[("pageToken",page.as_str()),("spaces","drive"),("restrictToMyDrive","true"),("fields",&format!("nextPageToken,newStartPageToken,changes(fileId,removed,file({FIELDS}))"))]).send()?)?.json()?;
            for c in v["changes"].as_array().context("Invalid change response")? {
                let id = c["fileId"]
                    .as_str()
                    .context("Missing changed file ID")?
                    .to_string();
                out.push((
                    id,
                    if c["removed"] == true
                        || c["file"]["trashed"] == true
                        || !supported(&c["file"])
                    {
                        None
                    } else {
                        Some(node(c["file"].clone())?)
                    },
                ));
            }
            if let Some(next) = v["nextPageToken"].as_str() {
                page = next.into();
            } else {
                return Ok((
                    out,
                    v["newStartPageToken"]
                        .as_str()
                        .context("Missing next cursor")?
                        .into(),
                ));
            }
        }
    }
    fn about(&self) -> Result<Value> {
        Ok(Self::checked(
            self.request(reqwest::Method::GET, &format!("{API}/about"))?
                .query(&[("fields", "user(displayName,emailAddress),storageQuota")])
                .send()?,
        )?
        .json()?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{io::Write, net::TcpListener, thread};

    struct Fixture {
        status: u16,
        headers: Vec<(&'static str, String)>,
        body: String,
    }
    type CapturedRequests = Arc<Mutex<Vec<(String, Vec<u8>)>>>;
    fn fixture(status: u16, body: Value) -> Fixture {
        Fixture {
            status,
            headers: vec![],
            body: body.to_string(),
        }
    }
    fn server(fixtures: Vec<Fixture>) -> (String, CapturedRequests, thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let requests = Arc::new(Mutex::new(vec![]));
        let captured = requests.clone();
        let thread = thread::spawn(move || {
            for response in fixtures {
                let (mut stream, _) = listener.accept().unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut bytes = vec![];
                let mut block = [0u8; 4096];
                let boundary = loop {
                    let count = stream.read(&mut block).unwrap();
                    assert!(count > 0);
                    bytes.extend_from_slice(&block[..count]);
                    if let Some(i) = bytes.windows(4).position(|b| b == b"\r\n\r\n") {
                        break i + 4;
                    }
                    assert!(bytes.len() < 65536);
                };
                let headers = String::from_utf8(bytes[..boundary].to_vec()).unwrap();
                let size = headers
                    .lines()
                    .find_map(|l| {
                        l.to_lowercase()
                            .strip_prefix("content-length:")
                            .and_then(|n| n.trim().parse::<usize>().ok())
                    })
                    .unwrap_or(0);
                while bytes.len() - boundary < size {
                    let count = stream.read(&mut block).unwrap();
                    assert!(count > 0);
                    bytes.extend_from_slice(&block[..count]);
                }
                captured.lock().push((headers, bytes[boundary..].to_vec()));
                let extra = response
                    .headers
                    .iter()
                    .map(|(k, v)| format!("{k}: {v}\r\n"))
                    .collect::<String>();
                write!(stream,"HTTP/1.1 {} Fixture\r\nContent-Length: {}\r\nContent-Type: application/json\r\nConnection: close\r\n{}\r\n{}",response.status,response.body.len(),extra,response.body).unwrap();
            }
        });
        (origin, requests, thread)
    }
    fn client(root: &Path, origin: &str) -> GoogleDrive {
        let mut drive = GoogleDrive::new(Arc::new(Auth::for_test(root.into()))).unwrap();
        drive.test_origin = Some(url::Url::parse(origin).unwrap());
        drive
    }
    fn file() -> Value {
        json!({"id":"file-1","name":"hello.txt","parents":["root"],"size":"100","version":"2","mimeType":"application/octet-stream"})
    }
    #[test]
    fn resumable_offsets_follow_server_acknowledgements() {
        assert_eq!(acknowledged(None).unwrap(), 0);
        assert_eq!(acknowledged(Some("bytes=0-8388607")).unwrap(), 8388608);
        assert!(acknowledged(Some("bytes=3-5")).is_err());
        assert!(acknowledged(Some("garbage")).is_err());
    }
    #[test]
    fn listings_paginate_and_hide_google_native_documents() {
        let (origin, requests, thread) = server(vec![
            fixture(
                200,
                json!({"nextPageToken":"page2","files":[file(),{"id":"doc","mimeType":"application/vnd.google-apps.document"}]}),
            ),
            fixture(200, json!({"files":[]})),
        ]);
        let root = tempfile::tempdir().unwrap();
        let drive = client(root.path(), &origin);
        let nodes = drive.list("root").unwrap();
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].id, "file-1");
        thread.join().unwrap();
        assert!(requests.lock()[1].0.contains("pageToken=page2"));
    }
    #[test]
    fn upload_resumes_acknowledged_offset_after_network_failure() {
        let (origin, requests, thread) = server(vec![
            Fixture {
                status: 200,
                headers: vec![(
                    "Location",
                    "https://www.googleapis.com/upload-session/1".into(),
                )],
                body: String::new(),
            },
            fixture(503, json!({})),
            Fixture {
                status: 308,
                headers: vec![("Range", "bytes=0-49".into())],
                body: String::new(),
            },
            fixture(200, file()),
        ]);
        let root = tempfile::tempdir().unwrap();
        let drive = client(root.path(), &origin);
        let path = root.path().join("content");
        std::fs::write(&path, vec![42u8; 100]).unwrap();
        let n = node(file()).unwrap();
        assert!(drive.upload(&n, &path, true).is_err());
        let result = drive.upload(&n, &path, true).unwrap();
        assert_eq!(result.id, n.id);
        thread.join().unwrap();
        let requests = requests.lock();
        assert_eq!(requests.len(), 4);
        assert!(
            requests[2]
                .0
                .to_lowercase()
                .contains("content-range: bytes */100")
        );
        assert!(
            requests[3]
                .0
                .to_lowercase()
                .contains("content-range: bytes 50-99/100")
        );
        assert_eq!(requests[3].1, vec![42u8; 50]);
        assert!(drive.progress.lock().is_none());
    }
    #[test]
    fn quota_errors_are_actionable() {
        let (origin, _, thread) = server(vec![fixture(403, json!({"error":"quota"}))]);
        let root = tempfile::tempdir().unwrap();
        let drive = client(root.path(), &origin);
        assert!(
            drive
                .get("file-1")
                .unwrap_err()
                .to_string()
                .contains("quota")
        );
        thread.join().unwrap();
    }
    #[test]
    fn ranges_use_exact_offsets_and_pin_etag_after_version_check() {
        let (origin, requests, thread) = server(vec![
            Fixture {
                status: 206,
                headers: vec![
                    ("Content-Range", "bytes 0-3/100".into()),
                    ("ETag", "\"v2\"".into()),
                ],
                body: "abcd".into(),
            },
            fixture(200, file()),
            Fixture {
                status: 206,
                headers: vec![
                    ("Content-Range", "bytes 4-7/100".into()),
                    ("ETag", "\"v2\"".into()),
                ],
                body: "efgh".into(),
            },
        ]);
        let root = tempfile::tempdir().unwrap();
        let drive = client(root.path(), &origin);
        let n = node(file()).unwrap();
        assert_eq!(drive.read_range(&n, 0, 4).unwrap(), b"abcd");
        assert_eq!(drive.read_range(&n, 4, 4).unwrap(), b"efgh");
        thread.join().unwrap();
        let requests = requests.lock();
        assert!(requests[0].0.to_lowercase().contains("range: bytes=0-3"));
        assert!(requests[2].0.to_lowercase().contains("if-match: \"v2\""));
    }
    #[test]
    fn ignored_ranges_and_truncated_bytes_are_rejected() {
        for (status, body) in [(200, "ignored"), (206, "abc")] {
            let (origin, _, thread) = server(vec![Fixture {
                status,
                headers: vec![("Content-Range", "bytes 0-3/100".into())],
                body: body.into(),
            }]);
            let root = tempfile::tempdir().unwrap();
            let drive = client(root.path(), &origin);
            assert!(drive.read_range(&node(file()).unwrap(), 0, 4).is_err());
            thread.join().unwrap();
        }
    }
    #[test]
    fn changed_cloud_revision_is_not_cached_as_old_content() {
        let mut changed = file();
        changed["version"] = json!("3");
        let (origin, _, thread) = server(vec![
            Fixture {
                status: 206,
                headers: vec![
                    ("Content-Range", "bytes 0-3/100".into()),
                    ("ETag", "\"v3\"".into()),
                ],
                body: "abcd".into(),
            },
            fixture(200, changed),
        ]);
        let root = tempfile::tempdir().unwrap();
        let drive = client(root.path(), &origin);
        assert!(
            drive
                .read_range(&node(file()).unwrap(), 0, 4)
                .unwrap_err()
                .to_string()
                .contains("changed")
        );
        thread.join().unwrap();
        assert!(drive.range_tags.lock().is_empty());
    }
}
