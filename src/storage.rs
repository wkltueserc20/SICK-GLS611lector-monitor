use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

pub fn now() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64()
}
pub fn image(p: &Path) -> bool {
    matches!(
        p.extension().and_then(|v| v.to_str()),
        Some("jpg" | "jpeg" | "png" | "bmp")
    )
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct CameraConfig {
    #[serde(default)]
    pub id: String,
    pub ip: String,
    pub label: String,
    #[serde(default)]
    pub ftp_port: Option<u16>,
    #[serde(default = "yes")]
    pub enabled: bool,
}
fn yes() -> bool {
    true
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub ftp_host: String,
    pub ftp_port: u16,
    pub web_port: u16,
    pub max_photos: usize,
    pub stale_seconds: u64,
    pub cameras: Vec<CameraConfig>,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            ftp_host: "0.0.0.0".into(),
            ftp_port: 2121,
            web_port: 8080,
            max_photos: 1000,
            stale_seconds: 300,
            cameras: ["左上", "右上", "左下", "右下"]
                .iter()
                .enumerate()
                .map(|(i, s)| CameraConfig {
                    id: format!("192.168.12.{}", 17 + i),
                    ip: format!("192.168.12.{}", 17 + i),
                    label: (*s).into(),
                    ftp_port: None,
                    enabled: true,
                })
                .collect(),
        }
    }
}
impl Config {
    pub fn load_for_ui(root: &Path) -> Result<Self> {
        let mut cfg: Self = serde_json::from_slice(&fs::read(root.join("config.json"))?)?;
        cfg.normalize_validate()?;
        Ok(cfg)
    }
    pub fn load(root: &Path) -> Result<Self> {
        fs::create_dir_all(root)?;
        let file = root.join("config.json");
        let mut cfg: Self = if file.exists() {
            serde_json::from_slice(&fs::read(&file)?)?
        } else {
            Self::default()
        };
        cfg.normalize_validate()?;
        // Persist generated identities so changing an IP cannot lose its history.
        replace_bytes(&file, &serde_json::to_vec_pretty(&cfg)?)?;
        Ok(cfg)
    }
    pub fn port(&self, c: &CameraConfig) -> u16 {
        c.ftp_port.unwrap_or(self.ftp_port)
    }
    pub fn ports(&self) -> std::collections::BTreeSet<u16> {
        self.cameras
            .iter()
            .filter(|c| c.enabled)
            .map(|c| self.port(c))
            .collect()
    }
    pub fn normalize_validate(&mut self) -> Result<()> {
        anyhow::ensure!(
            self.ftp_host.parse::<std::net::Ipv4Addr>().is_ok(),
            "FTP 綁定位址必須為 IPv4"
        );
        anyhow::ensure!(self.ftp_port > 0 && self.web_port > 0, "連接埠不可為 0");
        anyhow::ensure!(
            self.stale_seconds <= 604800,
            "未接收提醒請設定為 0–604800 秒，0 為關閉"
        );
        anyhow::ensure!(
            self.max_photos > 0 && self.max_photos <= 100000,
            "照片上限設定無效"
        );
        anyhow::ensure!(
            (1..=10).contains(&self.cameras.len()),
            "設備數量必須為 1–10 台（含停用設備）"
        );
        let mut ips = std::collections::HashSet::new();
        let mut ids = std::collections::HashSet::new();
        let default_port = self.ftp_port;
        for c in &mut self.cameras {
            c.ip =
                c.ip.trim()
                    .parse::<std::net::Ipv4Addr>()
                    .context("相機 IP 格式錯誤")?
                    .to_string();
            c.label = c.label.trim().to_string();
            anyhow::ensure!(
                !c.label.is_empty() && c.label.chars().count() <= 40,
                "設備名稱需為 1–40 字"
            );
            anyhow::ensure!(ips.insert(c.ip.clone()), "相機 IP 重複");
            if c.id.is_empty() {
                c.id = c.ip.clone();
            }
            anyhow::ensure!(
                c.id.len() <= 64
                    && c.id
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'.')
                    && !c.id.starts_with('.')
                    && !c.id.ends_with('.')
                    && c.id != "..",
                "設備識別碼格式錯誤"
            );
            let base = c.id.split('.').next().unwrap().to_ascii_uppercase();
            anyhow::ensure!(
                !matches!(base.as_str(), "CON" | "PRN" | "AUX" | "NUL")
                    && !(base.len() == 4
                        && (base.starts_with("COM") || base.starts_with("LPT"))
                        && base.ends_with(['1', '2', '3', '4', '5', '6', '7', '8', '9'])),
                "設備識別碼不可使用 Windows 保留名稱"
            );
            anyhow::ensure!(ids.insert(c.id.to_ascii_lowercase()), "設備識別碼重複");
            let port = c.ftp_port.unwrap_or(default_port);
            anyhow::ensure!(
                port > 0 && port != self.web_port && !(30000..=30010).contains(&port),
                "FTP 埠不可為 0、網頁埠或 30000–30010"
            );
        }
        Ok(())
    }
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Snapshot {
    pub file: String,
    pub name: String,
    pub time: f64,
    #[serde(default = "unknown")]
    pub result: String,
    #[serde(default)]
    pub codes: Vec<String>,
}
fn unknown() -> String {
    "unknown".into()
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Camera {
    #[serde(default)]
    pub id: String,
    pub ip: String,
    pub label: String,
    pub latest: Option<Snapshot>,
    #[serde(default)]
    pub ng: Option<Snapshot>,
    #[serde(default)]
    pub count: u64,
    #[serde(default)]
    pub retained: usize,
}
pub struct Store {
    pub root: PathBuf,
    pub config: Config,
    pub cameras: BTreeMap<String, Camera>,
}

fn walk(root: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    if !root.exists() {
        return Ok(());
    }
    for entry in fs::read_dir(root)? {
        let p = entry?.path();
        let meta = fs::symlink_metadata(&p)?;
        if meta.file_type().is_symlink() {
            continue;
        }
        if meta.is_dir() {
            walk(&p, out)?
        } else if image(&p) {
            out.push(p)
        }
    }
    Ok(())
}
pub fn classify(xml: &str) -> (String, Vec<String>) {
    let Ok(doc) = roxmltree::Document::parse(xml) else {
        return (unknown(), vec![]);
    };
    let Some(im) = doc.descendants().find(|n| n.has_tag_name("image")) else {
        return (unknown(), vec![]);
    };
    let noread = |s: &str| {
        s.chars()
            .filter(|c| c.is_alphanumeric())
            .collect::<String>()
            .eq_ignore_ascii_case("noread")
    };
    let results: Vec<_> = im
        .descendants()
        .filter(|n| {
            n.has_tag_name("result") && n.parent().is_some_and(|p| p.has_tag_name("symbol"))
        })
        .map(|n| n.text().unwrap_or("").trim())
        .collect();
    let codes: Vec<_> = results
        .iter()
        .filter(|s| !s.is_empty() && !noread(s))
        .map(|s| s.to_string())
        .collect();
    if !codes.is_empty() {
        return ("ok".into(), codes);
    }
    if im
        .descendants()
        .any(|n| noread(n.attribute("string").unwrap_or("")) || noread(n.text().unwrap_or("")))
    {
        return ("ng".into(), vec![]);
    }
    let processed = im.descendants().any(|n| {
        (n.has_tag_name("EVTimingAnalysis") && n.attribute("Name") == Some("DecoRun"))
            || (n.has_tag_name("box") && n.attribute("name") == Some("ImageProcessedByDecoder"))
            || (n.has_tag_name("text") && n.attribute("name") == Some("DPMAbort"))
    });
    let zero = im.descendants().any(|n| {
        n.has_tag_name("FramesDecoded")
            && n.parent().is_some_and(|p| p.has_tag_name("procd-img-trig"))
            && n.text().unwrap_or("").trim() == "0"
    });
    (
        if processed || zero {
            "ng".into()
        } else {
            unknown()
        },
        vec![],
    )
}
fn mtime(p: &Path) -> f64 {
    fs::metadata(p)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map_or(0., |t| t.as_secs_f64())
}
fn replace_bytes(path: &Path, bytes: &[u8]) -> Result<()> {
    let temp = path.with_extension("tmp");
    fs::write(&temp, bytes)?;
    // Windows cannot rename over an existing destination using std::fs::rename.
    // Keep the previous state as a recovery copy until the new state is in place.
    let backup = path.with_extension("bak");
    if backup.exists() {
        fs::remove_file(&backup)?
    }
    if path.exists() {
        fs::rename(path, &backup)?
    }
    if let Err(e) = fs::rename(&temp, path) {
        if backup.exists() {
            let _ = fs::rename(&backup, path);
        }
        return Err(e.into());
    }
    if backup.exists() {
        fs::remove_file(backup)?
    }
    Ok(())
}
impl Store {
    pub fn open(root: PathBuf, config: Config) -> Result<Self> {
        fs::create_dir_all(root.join("data"))?;
        fs::create_dir_all(root.join("logs"))?;
        let data = root.join("data/state.json");
        let backup = data.with_extension("bak");
        let saved: BTreeMap<String, Camera> = fs::read(&data)
            .or_else(|_| fs::read(&backup))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        let cameras = config
            .cameras
            .iter()
            .map(|c| {
                let mut cam = saved
                    .values()
                    .find(|s| s.id == c.id || (s.id.is_empty() && s.ip == c.id))
                    .cloned()
                    .unwrap_or(Camera {
                        id: c.id.clone(),
                        ip: c.ip.clone(),
                        label: c.label.clone(),
                        latest: None,
                        ng: None,
                        count: 0,
                        retained: 0,
                    });
                cam.label = c.label.clone();
                cam.id = c.id.clone();
                cam.ip = c.ip.clone();
                (c.ip.clone(), cam)
            })
            .collect();
        let mut s = Self {
            root,
            config,
            cameras,
        };
        let ips: Vec<_> = s.cameras.keys().cloned().collect();
        for ip in ips {
            fs::create_dir_all(s.root.join("photos").join(s.folder(&ip)?))?;
            let mut files = s.files(&ip)?;
            files.sort_by(|a, b| mtime(a).total_cmp(&mtime(b)));
            for p in files {
                s.apply(&ip, &p)?;
            }
            s.prune(&ip)?;
        }
        s.save()?;
        Ok(s)
    }
    pub fn files(&self, ip: &str) -> Result<Vec<PathBuf>> {
        let mut v = vec![];
        let folder = self.folder(ip)?;
        walk(&self.root.join("photos").join(folder), &mut v)?;
        walk(&self.root.join("ng_photos").join(folder), &mut v)?;
        Ok(v)
    }
    pub fn folder(&self, ip: &str) -> Result<&str> {
        Ok(&self.cameras.get(ip).context("Unknown IP")?.id)
    }
    pub fn accepts(&self, ip: &str, port: u16) -> bool {
        self.config
            .cameras
            .iter()
            .any(|c| c.enabled && c.ip == ip && self.config.port(c) == port)
    }
    pub fn save_config(&self, mut cfg: Config) -> Result<Config> {
        cfg.normalize_validate()?;
        // A port already used outside this process must be fixed before saving.
        for port in cfg.ports() {
            if cfg.ftp_host != self.config.ftp_host || !self.config.ports().contains(&port) {
                std::net::TcpListener::bind((cfg.ftp_host.as_str(), port))
                    .with_context(|| format!("FTP 埠 {port} 已被使用或無法綁定"))?;
            }
        }
        if cfg.web_port != self.config.web_port {
            std::net::TcpListener::bind(("127.0.0.1", cfg.web_port)).context("網頁埠已被使用")?;
        }
        let bytes = serde_json::to_vec_pretty(&cfg)?;
        let file = self.root.join("config.json");
        if file.exists() {
            replace_bytes(&self.root.join("config.previous.json"), &fs::read(&file)?)?;
        }
        replace_bytes(&file, &bytes)?;
        Ok(cfg)
    }
    pub fn save(&self) -> Result<()> {
        replace_bytes(
            &self.root.join("data/state.json"),
            &serde_json::to_vec_pretty(&self.cameras)?,
        )
    }
    pub fn log(&self, msg: &str) {
        use std::io::Write;
        let p = self.root.join("logs/receiver-rust.log");
        if fs::metadata(&p)
            .map(|m| m.len() > 2_000_000)
            .unwrap_or(false)
        {
            let _ = fs::remove_file(p.with_extension("old"));
            let _ = fs::rename(&p, p.with_extension("old"));
        }
        if let Ok(mut f) = fs::OpenOptions::new().create(true).append(true).open(p) {
            let _ = writeln!(f, "{:.3} {}", now(), msg);
        }
    }
    pub fn record(&mut self, ip: &str, path: &Path) -> Result<()> {
        if image(path) {
            let ext = path.extension().unwrap().to_string_lossy();
            let name = format!("latest-{}.{}", self.folder(ip)?, ext);
            replace_bytes(&self.root.join("data").join(&name), &fs::read(path)?)?;
            let cam = self.cameras.get_mut(ip).context("Unknown IP")?;
            cam.count += 1;
            cam.latest = Some(Snapshot {
                file: name,
                name: path.file_name().unwrap().to_string_lossy().into(),
                time: now(),
                result: unknown(),
                codes: vec![],
            });
            self.apply(ip, path)?;
        } else if path.extension().is_some_and(|e| e == "xml") {
            for ext in ["jpg", "jpeg", "png", "bmp"] {
                let p = path.with_extension(ext);
                if p.exists() {
                    self.apply(ip, &p)?;
                    break;
                }
            }
        }
        self.prune(ip)?;
        self.save()?;
        self.log(&format!("Received {} {}", ip, path.display()));
        Ok(())
    }
    pub fn apply(&mut self, ip: &str, path: &Path) -> Result<()> {
        if !path.exists() {
            return Ok(());
        }
        let xml = fs::read_to_string(path.with_extension("xml")).unwrap_or_default();
        let (result, codes) = classify(&xml);
        let name = path
            .file_name()
            .context("Filename")?
            .to_string_lossy()
            .to_string();
        let cam = self.cameras.get_mut(ip).context("Unknown IP")?;
        if let Some(latest) = &mut cam.latest {
            if latest.name == name {
                latest.result = result.clone();
                latest.codes = codes;
            }
        }
        if result != "ng" {
            return Ok(());
        }
        let received = mtime(path);
        if cam.ng.as_ref().is_none_or(|n| received > n.time) {
            let filename = format!(
                "ng-{}.{}",
                cam.id,
                path.extension().unwrap().to_string_lossy()
            );
            replace_bytes(&self.root.join("data").join(&filename), &fs::read(path)?)?;
            cam.ng = Some(Snapshot {
                file: filename,
                name: name.clone(),
                time: received,
                result: "ng".into(),
                codes: vec![],
            });
        }
        let ngroot = self.root.join("ng_photos").join(&cam.id);
        if !path.starts_with(&ngroot) {
            let relative = path.strip_prefix(self.root.join("photos").join(&cam.id))?;
            let mut dest = ngroot.join(relative);
            fs::create_dir_all(dest.parent().unwrap())?;
            if dest.exists() || dest.with_extension("xml").exists() {
                dest.set_file_name(format!(
                    "{}_{}.{}",
                    path.file_stem().unwrap().to_string_lossy(),
                    uuid::Uuid::new_v4(),
                    path.extension().unwrap().to_string_lossy()
                ));
            }
            fs::rename(path, &dest)?;
            let meta = path.with_extension("xml");
            if meta.exists() {
                if let Err(e) = fs::rename(&meta, dest.with_extension("xml")) {
                    let _ = fs::rename(&dest, path);
                    return Err(e.into());
                }
            }
            if dest.file_name() != path.file_name() {
                let new = dest.file_name().unwrap().to_string_lossy().to_string();
                if let Some(x) = &mut cam.latest {
                    if x.name == name {
                        x.name = new.clone()
                    }
                }
                if let Some(x) = &mut cam.ng {
                    if x.name == name {
                        x.name = new
                    }
                }
            }
        }
        Ok(())
    }
    pub fn prune(&mut self, ip: &str) -> Result<()> {
        let mut files = self.files(ip)?;
        files.sort_by(|a, b| mtime(b).total_cmp(&mtime(a)).then_with(|| b.cmp(a)));
        for p in files.iter().skip(self.config.max_photos) {
            fs::remove_file(p)?;
            let xml = p.with_extension("xml");
            if xml.exists() {
                fs::remove_file(xml)?;
            }
        }
        self.cameras.get_mut(ip).unwrap().retained = files.len().min(self.config.max_photos);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn variable_devices_and_validation() {
        let mut cfg = Config::default();
        cfg.cameras.truncate(1);
        cfg.normalize_validate().unwrap();
        let first = cfg.cameras[0].clone();
        for n in 2..=10 {
            let mut c = first.clone();
            c.id = format!("device-{n}");
            c.ip = format!("10.1.0.{n}");
            c.ftp_port = Some(2100 + n);
            cfg.cameras.push(c);
        }
        cfg.normalize_validate().unwrap();
        cfg.cameras.push(first.clone());
        assert!(cfg.normalize_validate().is_err());
        cfg.cameras.pop();
        cfg.cameras[1].ftp_port = Some(30000);
        assert!(cfg.normalize_validate().is_err());
        cfg.cameras[1].ftp_port = Some(8080);
        assert!(cfg.normalize_validate().is_err());
        cfg.cameras[1].ftp_port = Some(2121);
        cfg.cameras[1].ip = first.ip;
        assert!(cfg.normalize_validate().is_err());
    }
    #[test]
    fn stable_history_after_ip_change() {
        let root = std::env::temp_dir().join(uuid::Uuid::new_v4().to_string());
        let mut cfg = Config::default();
        cfg.cameras.truncate(1);
        let old = cfg.cameras[0].ip.clone();
        let mut s = Store::open(root.clone(), cfg.clone()).unwrap();
        let p = root.join("photos").join(&old).join("first.jpg");
        fs::write(&p, b"history").unwrap();
        fs::write(
            p.with_extension("xml"),
            "<camera><image><symbol><result>NoRead</result></symbol></image></camera>",
        )
        .unwrap();
        s.record(&old, &p).unwrap();
        let previous = s.cameras[&old].ng.clone().unwrap();
        drop(s);
        cfg.cameras[0].ip = "10.2.0.17".into();
        let s = Store::open(root.clone(), cfg).unwrap();
        assert_eq!(
            s.cameras["10.2.0.17"].ng.as_ref().unwrap().file,
            previous.file
        );
        assert_eq!(s.files("10.2.0.17").unwrap().len(), 1);
        assert!(s.accepts("10.2.0.17", 2121));
        assert!(!s.accepts(&old, 2121));
        drop(s);
        let _ = fs::remove_dir_all(root);
    }
    #[test]
    fn no_read_not_profiler() {
        let x="<camera><image><EVTimingAnalysis Name='DecoRun'/><DecoderProfiler><procd-img-trig><FramesDecoded>1</FramesDecoded></procd-img-trig></DecoderProfiler></image></camera>";
        assert_eq!(classify(x).0, "ng");
        assert_eq!(classify("bad").0, "unknown");
        assert_eq!(
            classify("<camera><image><symbol><result>001067</result></symbol></image></camera>").1,
            vec!["001067"]
        );
    }
    #[test]
    fn migrate_and_prune() {
        let root = std::env::temp_dir().join(uuid::Uuid::new_v4().to_string());
        let cfg = Config {
            max_photos: 2,
            ..Config::default()
        };
        let ip = "192.168.12.17";
        let p = root.join("photos").join(ip);
        fs::create_dir_all(&p).unwrap();
        fs::write(p.join("old.jpg"), b"image").unwrap();
        fs::write(
            p.join("old.xml"),
            "<camera><image><symbol><result>NoRead</result></symbol></image></camera>",
        )
        .unwrap();
        let mut s = Store::open(root.clone(), cfg).unwrap();
        assert!(root.join("ng_photos").join(ip).join("old.jpg").exists());
        let ng = s.cameras[ip].ng.clone().unwrap();
        for n in 0..3 {
            let f = p.join(format!("{n}.jpg"));
            fs::write(&f, b"new").unwrap();
            s.record(ip, &f).unwrap();
        }
        assert_eq!(s.files(ip).unwrap().len(), 2);
        assert_eq!(s.cameras[ip].ng.as_ref().unwrap().name, ng.name);
        assert!(root.join("data").join(ng.file).exists());
        drop(s);
        let _ = fs::remove_dir_all(root);
    }
}
